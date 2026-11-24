//! Natural growth on the map: trees grow (saplings planted by foresters become fellable), stumps
//! of felled trees rot away and fish grounds regenerate slowly (docs/game_design.md §3).
//!
//! The map is swept row by row so the cost is spread over the hour: in every tick the rows
//! `y ≡ tick (mod TICKS_PER_HOUR)` are processed, so each tile is visited once per game hour.
//! Trees grow by `GameRules::tree_growth_per_hour` × the season's growth factor (none in
//! winter); fish tiles regain `GameRules::fish_regeneration_per_day` units in the first hour of
//! each day, up to `GameRules::max_fish_per_tile`; a stump disappears after
//! `GameRules::stump_decay_hours`. A tile is marked dirty for renderers only when a tree reaches
//! a new visual stage ([`tree_stage`]) or a stump vanishes.

use glam::IVec2;

use crate::defs::{Defs, GameRules};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;
use crate::world::{DepositKind, Vegetation};

/// Growth below which a tree is drawn as a sapling.
pub const SAPLING_GROWTH: u8 = 64;

/// Visual stage of a tree: 0 = sapling, 1 = young tree, 2 = grown (fellable).
pub fn tree_stage(growth: u8, rules: &GameRules) -> u8 {
    if growth < SAPLING_GROWTH {
        0
    } else if growth < rules.fellable_tree_growth {
        1
    } else {
        2
    }
}

/// Tree growth per game hour in the current season.
pub fn tree_growth_step(state: &GameState, defs: &Defs) -> u8 {
    let rules = defs.rules();
    let factor = f64::from(*rules.season_growth.get(state.clock.season()));
    (f64::from(rules.tree_growth_per_hour) * factor).round().clamp(0.0, 255.0) as u8
}

/// Pipeline step 4 (nature part): grows the trees and regenerates the fish of this tick's rows.
pub(crate) fn run_nature(state: &mut GameState, defs: &Defs) {
    let rules = defs.rules();
    let step = tree_growth_step(state, defs);
    let regenerate = state.clock.hour() == 0;
    let (w, h) = (state.world.width(), state.world.height());
    let period = TICKS_PER_HOUR as i32;
    let first = (state.clock.tick % TICKS_PER_HOUR) as i32;
    let mut y = first;
    while y < h {
        for x in 0..w {
            let p = IVec2::new(x, y);
            let tile = *state.world.tile(p);
            if let Vegetation::Tree { kind, growth } = tile.vegetation
                && growth < u8::MAX
                && step > 0
            {
                let grown = growth.saturating_add(step);
                let vegetation = Vegetation::Tree { kind, growth: grown };
                if tree_stage(grown, rules) != tree_stage(growth, rules) {
                    state.world.tile_mut(p).vegetation = vegetation;
                } else {
                    state.world.tile_mut_quiet(p).vegetation = vegetation;
                }
            }
            if let Vegetation::Stump { hours } = tile.vegetation {
                let hours = hours.saturating_add(1);
                if hours >= rules.stump_decay_hours {
                    state.world.tile_mut(p).vegetation = Vegetation::None;
                } else {
                    state.world.tile_mut_quiet(p).vegetation = Vegetation::Stump { hours };
                }
            }
            if regenerate
                && let Some(d) = tile.deposit
                && d.kind == DepositKind::Fish
                && d.amount < rules.max_fish_per_tile
            {
                let amount = d.amount.saturating_add(rules.fish_regeneration_per_day).min(rules.max_fish_per_tile);
                if let Some(deposit) = state.world.tile_mut_quiet(p).deposit.as_mut() {
                    deposit.amount = amount;
                }
            }
        }
        y += period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{defs, flat_state, run};
    use crate::time::{GameClock, TICKS_PER_DAY, TICKS_PER_SEASON};
    use crate::world::{Deposit, Terrain, TreeKind};

    #[test]
    fn trees_grow_through_their_stages() {
        let d = defs();
        let mut state = flat_state(16);
        let p = IVec2::new(3, 3);
        state.world.tile_mut(p).vegetation = Vegetation::Tree { kind: TreeKind::Birch, growth: 0 };
        state.world.take_dirty_chunks();
        assert_eq!(tree_growth_step(&state, d), 3);
        run(&mut state, TICKS_PER_HOUR * 22);
        let Vegetation::Tree { growth, .. } = state.world().tile(p).vegetation else { panic!("tree vanished") };
        assert_eq!(growth, 66, "22 hours at 3 per hour");
        assert_eq!(tree_stage(growth, d.rules()), 1);
        assert!(!state.take_dirty_chunks().is_empty(), "the stage change marks the chunk dirty");
        run(&mut state, TICKS_PER_HOUR * 100);
        assert_eq!(state.world().tile(p).vegetation, Vegetation::Tree { kind: TreeKind::Birch, growth: 255 });
        // No growth in winter.
        state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON);
        assert_eq!(tree_growth_step(&state, d), 0);
    }

    #[test]
    fn stumps_rot_away() {
        let d = defs();
        let mut state = flat_state(16);
        let p = IVec2::new(4, 4);
        state.world.tile_mut(p).vegetation = Vegetation::Stump { hours: 0 };
        state.world.take_dirty_chunks();
        run(&mut state, TICKS_PER_HOUR * (u64::from(d.rules().stump_decay_hours) - 1));
        assert!(state.world().tile(p).vegetation.is_stump());
        assert!(state.take_dirty_chunks().is_empty(), "ageing is invisible");
        run(&mut state, TICKS_PER_HOUR);
        assert_eq!(state.world().tile(p).vegetation, Vegetation::None);
        assert!(!state.take_dirty_chunks().is_empty(), "the vanished stump is redrawn");
    }

    #[test]
    fn fish_regenerates_once_per_day_up_to_the_maximum() {
        let d = defs();
        let mut state = flat_state(16);
        let p = IVec2::new(2, 2);
        state.world.tile_mut(p).terrain = Terrain::ShallowWater;
        state.world.tile_mut(p).deposit = Some(Deposit { kind: DepositKind::Fish, amount: 10 });
        run(&mut state, TICKS_PER_DAY * 3);
        let amount = state.world().tile(p).deposit.map(|d| d.amount);
        assert_eq!(amount, Some(10 + 3 * d.rules().fish_regeneration_per_day));
        state.world.tile_mut(p).deposit = Some(Deposit { kind: DepositKind::Fish, amount: d.rules().max_fish_per_tile });
        run(&mut state, TICKS_PER_DAY);
        assert_eq!(state.world().tile(p).deposit.map(|d| d.amount), Some(d.rules().max_fish_per_tile));
    }
}
