//! Friendships (docs/game_design.md §6 Beziehungen).
//!
//! Once per game hour (`run_hour`, population step) the citizens who are together at a
//! place get to know each other: co-workers inside their workplace, pupils at school, guests
//! of a tavern or church in their free time, and the households sharing a house. Each such
//! citizen with room for a friend meets one other citizen of the group (chosen by a hash of
//! the hour and the citizen) and they become friends with `friendship_chance_permille` (a hash
//! roll as well, so friendships draw no random numbers and shift no other roll). Members of
//! the same household are family, not friends. Friendships are mutual and end only with death
//! or emigration; every citizen keeps at most `max_friends`. Each friend raises the social need
//! by `friend_social_bonus`.

use std::collections::BTreeMap;

use crate::buildings::BuildingId;
use crate::defs::Defs;
use crate::state::GameState;

use super::movement::{Destination, Purpose};
use super::{CitizenId, LifeStage};

/// SplitMix64 finaliser (hash rolls).
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Raw id bits of a citizen (index and generation) for hashing.
fn key(id: CitizenId) -> u64 {
    (u64::from(id.generation()) << 32) | u64::from(id.index())
}

/// The place where citizen `c` meets others right now, if any (see the module documentation).
fn meeting_place(state: &GameState, id: CitizenId) -> Option<BuildingId> {
    let c = state.population.citizens.get(id)?;
    let inside = c.motion.inside?;
    if c.job.is_some_and(|j| j.building == inside) {
        return Some(inside);
    }
    match c.motion.dest {
        Some(Destination::Building { building, purpose: Purpose::School | Purpose::Leisure | Purpose::Home })
            if building == inside =>
        {
            Some(inside)
        }
        _ => None,
    }
}

/// True if `a` and `b` can still become friends.
fn can_befriend(state: &GameState, max: usize, a: CitizenId, b: CitizenId) -> bool {
    let (Some(ca), Some(cb)) = (state.population.citizens.get(a), state.population.citizens.get(b)) else { return false };
    a != b
        && ca.household != cb.household
        && ca.friends.len() < max
        && cb.friends.len() < max
        && !ca.friends.contains(&b)
        && (ca.stage == LifeStage::Child) == (cb.stage == LifeStage::Child)
}

/// Makes `a` and `b` friends (both lists stay sorted).
fn befriend(state: &mut GameState, a: CitizenId, b: CitizenId) {
    for (x, y) in [(a, b), (b, a)] {
        if let Some(c) = state.population.citizens.get_mut(x)
            && let Err(i) = c.friends.binary_search(&y)
        {
            c.friends.insert(i, y);
        }
    }
}

/// Hourly friendship step (see the module documentation).
pub(crate) fn run_hour(state: &mut GameState, defs: &Defs) {
    let rules = &defs.rules().population;
    let max = usize::from(rules.max_friends);
    if max == 0 || rules.friendship_chance_permille == 0 {
        return;
    }
    let mut groups: BTreeMap<BuildingId, Vec<CitizenId>> = BTreeMap::new();
    for (id, c) in state.population.citizens() {
        if c.friends.len() < max
            && let Some(place) = meeting_place(state, id)
        {
            groups.entry(place).or_default().push(id);
        }
    }
    let hour = state.clock.total_hours();
    for members in groups.values().filter(|m| m.len() >= 2) {
        let n = members.len() as u64;
        for (i, &a) in members.iter().enumerate() {
            let roll = mix(hour ^ key(a).rotate_left(17));
            let offset = 1 + roll % (n - 1);
            let b = members[((i as u64 + offset) % n) as usize];
            if (roll >> 32) % 1000 < u64::from(rules.friendship_chance_permille) && can_befriend(state, max, a, b) {
                befriend(state, a, b);
            }
        }
    }
}

/// Removes citizen `id` from the friend lists of its friends (death, emigration).
pub(crate) fn forget(state: &mut GameState, id: CitizenId) {
    let friends = state.population.citizens.get(id).map(|c| c.friends.clone()).unwrap_or_default();
    for f in friends {
        if let Some(c) = state.population.citizens.get_mut(f) {
            c.friends.retain(|x| *x != id);
        }
    }
    if let Some(c) = state.population.citizens.get_mut(id) {
        c.friends.clear();
    }
}

/// Social bonus of citizen `id` from its friends.
pub(crate) fn social_bonus(defs: &Defs, friends: usize) -> u8 {
    let rules = &defs.rules().population;
    u8::try_from(friends.min(usize::from(rules.max_friends)) * usize::from(rules.friend_social_bonus)).unwrap_or(u8::MAX)
}

/// Checks the friend lists of a loaded state: existing, sorted, mutual, not oneself, at most
/// `max_friends`.
pub(crate) fn validate(state: &GameState, defs: &Defs) -> Result<(), String> {
    let max = usize::from(defs.rules().population.max_friends);
    for (id, c) in state.population.citizens() {
        if c.friends.len() > max || c.friends.windows(2).any(|w| w[0] >= w[1]) || c.friends.contains(&id) {
            return Err(format!("citizen {id:?} has a malformed friend list"));
        }
        for f in &c.friends {
            let mutual = state.population.citizens.get(*f).is_some_and(|o| o.friends.binary_search(&id).is_ok());
            if !mutual {
                return Err(format!("friendship of {id:?} and {f:?} is not mutual"));
            }
        }
    }
    Ok(())
}
