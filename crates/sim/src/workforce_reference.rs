//! The job allocation as it was before the step-7 optimisation, kept verbatim as the reference
//! the optimised [`super::allocate`] must match exactly (differential tests in
//! `super::tests`). Test-only.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::{Demand, collect_demands, educated_in_plain_job, set_targets};
use crate::buildings::BuildingId;
use crate::defs::Defs;
use crate::grid::TileRect;
use crate::population::{CitizenId, Employment};
use crate::state::GameState;

fn origin_of(state: &GameState, defs: &Defs, citizen: CitizenId) -> TileRect {
    let home = state
        .population
        .citizen(citizen)
        .and_then(|c| state.population.household(c.household()))
        .and_then(|h| h.home())
        .and_then(|b| state.buildings.get(b))
        .map(|b| b.rect());
    home.unwrap_or_else(|| super::camp_rect(state, defs))
}

/// The reference allocation.
pub(super) fn allocate(state: &mut GameState, defs: &Defs) {
    let mut demands = collect_demands(state, defs);
    set_targets(state, &mut demands);
    release_surplus(state, defs, &demands);
    fill_vacancies(state, defs, &demands);
    for (id, b) in state.buildings.iter_mut() {
        let n = state.population.workers_of(id).len();
        b.set_staffed(u8::try_from(n).unwrap_or(u8::MAX));
    }
    state.workforce.dirty = false;
    state.workforce.refresh_counts(&state.population);
}

fn release_surplus(state: &mut GameState, defs: &Defs, demands: &[Demand]) {
    let employed: Vec<(CitizenId, Employment, bool, bool)> = state
        .population
        .citizens()
        .filter_map(|(id, c)| c.job().map(|j| (id, j, c.can_take_job(), c.is_educated())))
        .collect();
    let by_building: BTreeMap<BuildingId, &Demand> = demands.iter().map(|d| (d.building, d)).collect();
    for (id, job, can_work, educated) in employed {
        let keep = can_work
            && by_building
                .get(&job.building)
                .is_some_and(|d| d.category == job.category && (educated || !d.educated_only) && d.target > 0);
        if !keep {
            state.population.set_job(id, None);
        }
    }
    for d in demands {
        let workers = state.population.workers_of(d.building).to_vec();
        let surplus = workers.len().saturating_sub(usize::from(d.target));
        if surplus == 0 {
            continue;
        }
        let mut ranked: Vec<(Reverse<i32>, Reverse<CitizenId>)> = workers
            .iter()
            .map(|w| (Reverse(origin_of(state, defs, *w).distance_to_rect(&d.rect)), Reverse(*w)))
            .collect();
        ranked.sort();
        for (_, Reverse(w)) in ranked.into_iter().take(surplus) {
            state.population.set_job(w, None);
        }
    }
}

struct Candidate {
    id: CitizenId,
    educated: bool,
    origin: TileRect,
}

fn fill_vacancies(state: &mut GameState, defs: &Defs, demands: &[Demand]) {
    let mut pool: Vec<Candidate> = state
        .population
        .citizens()
        .filter(|(_, c)| c.can_take_job() && c.job().is_none())
        .map(|(id, c)| Candidate { id, educated: c.is_educated(), origin: origin_of(state, defs, id) })
        .collect();
    for d in demands.iter().filter(|d| d.educated_only) {
        while state.population.workers_of(d.building).len() < usize::from(d.target) {
            if let Some(i) = nearest(&pool, &d.rect, |c| c.educated) {
                let c = pool.remove(i);
                state.population.set_job(c.id, Some(Employment { building: d.building, category: d.category }));
            } else if let Some(w) = educated_in_plain_job(state, demands) {
                state.population.set_job(w, Some(Employment { building: d.building, category: d.category }));
            } else {
                break;
            }
        }
    }
    for d in demands.iter().filter(|d| !d.educated_only) {
        while state.population.workers_of(d.building).len() < usize::from(d.target) {
            let pick = nearest(&pool, &d.rect, |c| !c.educated).or_else(|| nearest(&pool, &d.rect, |c| c.educated));
            let Some(i) = pick else { break };
            let c = pool.remove(i);
            state.population.set_job(c.id, Some(Employment { building: d.building, category: d.category }));
        }
    }
}

fn nearest(pool: &[Candidate], rect: &TileRect, filter: impl Fn(&Candidate) -> bool) -> Option<usize> {
    pool.iter()
        .enumerate()
        .filter(|(_, c)| filter(c))
        .min_by_key(|(_, c)| (c.origin.distance_to_rect(rect), c.id))
        .map(|(i, _)| i)
}
