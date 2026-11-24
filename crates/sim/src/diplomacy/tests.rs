//! Tests of the kingdoms, diplomacy and caravans.

use super::*;
use crate::command::{Command, RejectReason};
use crate::defs::Defs;
use crate::economy::{TradeOrder, storage};
use crate::state::{GameSettings, GameState};
use crate::territory::Territory;
use crate::test_support::{defs, exec, flat_state, rejection, run, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::WorldSize;

/// A kingdom with no land, 100 inhabitants and nothing remembered (unit tests of single
/// functions).
pub(crate) fn blank_kingdom(defs: &Defs) -> Kingdom {
    let resources = defs.resources().len();
    Kingdom {
        name: "Testreich".into(),
        ruler: "Test".into(),
        title: "König".into(),
        personality: Personality::Peaceful,
        color: 0,
        seat: IVec2::ZERO,
        radius: 8,
        ceded: 0,
        population: 100,
        treasury_milli: 0,
        stock_milli: vec![0; resources],
        production_milli: vec![0; resources],
        farm_milli: vec![0; resources],
        land: vec![0; crate::defs::LandSource::ALL.len()],
        territory_tiles: 0,
        soldiers: 0,
        food_percent: 100,
        memories: Vec::new(),
        treaties: Vec::new(),
        war: None,
        structures: Vec::new(),
        borders_player: false,
        last_state: RelationState::Neutral,
        next_caravan_tick: 0,
        next_initiative_day: 0,
        next_demand_day: 0,
    }
}


fn res(key: &str) -> ResourceId {
    defs().resource_id(key).expect("resource")
}

/// Sum of `resource` over the player's buildings, goods in transit, kingdoms and caravans.
fn goods_everywhere(state: &GameState, resource: ResourceId) -> u64 {
    let buildings: u64 = state.buildings.iter().map(|(_, b)| u64::from(b.inventory().get(resource))).sum();
    let transit: u64 = state.logistics.goods_in_transit().iter().filter(|m| m.resource == resource).map(|m| u64::from(m.amount)).sum();
    let kingdoms: u64 = state.diplomacy.kingdoms.iter().map(|k| k.stock_milli[resource.index()] / 1000).sum();
    let caravans: u64 = state.diplomacy.caravans.iter().map(|c| u64::from(c.carried(resource))).sum();
    buildings + transit + kingdoms + caravans
}

/// Gold of the player, the kingdoms and the caravans in milli-gold.
fn gold_everywhere(state: &GameState) -> i64 {
    state.economy.treasury_milli()
        + state.diplomacy.kingdoms.iter().map(|k| k.treasury_milli).sum::<i64>()
        + state.diplomacy.caravans.iter().map(|c| c.purse_milli).sum::<i64>()
}

/// A 128 × 128 flat map: the player owns the disc of radius 40 around (24, 24) and has a
/// staffed trading post at (20, 20) besides the fixture depot (no headquarters, so every
/// storage counts); kingdoms sit at `sites`.
pub(crate) fn kingdom_fixture(sites: &[IVec2]) -> GameState {
    let d = defs();
    let mut state = flat_state(128);
    state.territory = Territory::new(128, 128);
    state.territory.claim(TileRect::tile(IVec2::new(24, 24)), 40);
    state.world.settlement_sites = sites.to_vec();
    setup::create_kingdoms(&mut state, d);
    let post = spawn(&mut state, "trading_post", 20, 20);
    storage::refresh_stock(&mut state, d);
    crate::navigation::sync(&mut state, d);
    crate::test_support::run_until(&mut state, 2 * TICKS_PER_HOUR, |s| s.building(post).is_some_and(|b| b.staffed() > 0));
    assert!(crate::economy::trade::active_trading_post(&state, d).is_some(), "the post trades");
    state
}

const PARTNER: KingdomId = KingdomId(0);

#[test]
fn kingdoms_sit_far_from_the_player_on_their_own_land() {
    let d = defs();
    let state = GameState::new(d, GameSettings::new(d, 7, WorldSize::Medium));
    state.validate(d).expect("valid state");
    assert_eq!(state.diplomacy.len(), 4, "a medium map holds four kingdoms");
    let mut names: Vec<&str> = state.diplomacy.kingdoms.iter().map(|k| k.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 4, "distinct realms");
    for (id, k) in state.diplomacy.kingdoms() {
        assert_eq!(usize::from(k.color), id.index(), "distinct colours");
        assert!((k.seat - state.world.start).as_vec2().length() >= 24.0, "{} sits far from the player", k.name);
        assert_eq!(k.territory_tiles, state.diplomacy.land.count_owned(id));
        assert!(k.territory_tiles > 300, "{} owns land", k.name);
        assert!(k.structures.len() >= 8, "{} has an established town", k.name);
        assert_eq!(d.building(k.structures[0].kind).key, "castle", "the seat comes first");
        assert!(k.population >= 80 && k.soldiers > 0 && k.treasury() > 0);
        assert!(k.ruler_text().contains(" von "));
    }
    // No tile belongs to the player and a kingdom.
    assert!(state.territory.owned_positions().all(|p| !state.diplomacy.land.is_foreign(p)));
    // Structures block walkers.
    let s = state.diplomacy.kingdoms[0].structures[0];
    assert!(!state.navigation.is_walkable(s.origin));
    // Renderers find the structures by rectangle.
    let rect = s.rect(d);
    assert!(state.diplomacy.structures_in(d, rect).any(|(k, found)| k == PARTNER && *found == s));
}

#[test]
fn the_player_cannot_build_on_foreign_land() {
    let d = defs();
    let state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let k = &state.diplomacy.kingdoms[0];
    // A free tile of the kingdom next to its border.
    let spot = TileRect::new(k.seat - IVec2::splat(i32::from(k.radius)), IVec2::splat(2 * i32::from(k.radius)))
        .tiles()
        .find(|p| state.diplomacy.land.owner(*p) == Some(PARTNER) && !state.diplomacy.land.has_structure(*p))
        .expect("free foreign tile");
    let well = d.building_id("well").unwrap();
    let check = crate::buildings::check_placement(&state, d, well, spot, crate::grid::Rotation::default());
    assert!(check.errors.contains(&crate::buildings::PlacementError::ForeignKingdom), "{:?}", check.errors);
    assert!(check.errors.iter().any(|e| e.to_string() == "Dieses Land gehört einem anderen Königreich."));
}

#[test]
fn kingdoms_grow_their_people_land_and_towns() {
    let d = defs();
    let mut state = GameState::new(d, GameSettings::new(d, 7, WorldSize::Small).with_events(false));
    let before: Vec<(u32, u16, usize)> = state.diplomacy.kingdoms.iter().map(|k| (k.population, k.radius, k.structures.len())).collect();
    run(&mut state, 30 * TICKS_PER_DAY);
    let after: Vec<(u32, u16, usize)> = state.diplomacy.kingdoms.iter().map(|k| (k.population, k.radius, k.structures.len())).collect();
    assert!(before.iter().zip(&after).any(|(b, a)| a.0 > b.0 && a.1 > b.1 && a.2 > b.2), "{before:?} -> {after:?}");
    for k in &state.diplomacy.kingdoms {
        assert!(k.food_percent >= 90, "{} is fed ({} %)", k.name, k.food_percent);
    }
    state.validate(d).expect("valid state");
}

#[test]
fn judgements_are_deterministic_and_explained() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let propose = DiplomaticAction::ProposeTreaty { treaty: TreatyKind::TradeAgreement };
    relations::set_relation(&mut state, d, PARTNER, 60);
    let a = actions::judge(&state, d, PARTNER, &propose).unwrap();
    let b = actions::judge(&state.clone(), d, PARTNER, &propose).unwrap();
    assert_eq!(a, b, "same state, same judgement");
    assert!(a.accepted, "{a:?}");
    assert!(a.reasons.iter().any(|r| r.label.starts_with("Beziehung") && r.points == 60));
    assert_eq!(a.score, a.reasons.iter().map(|r| r.points).sum::<i32>());
    relations::set_relation(&mut state, d, PARTNER, -40);
    let declined = actions::judge(&state, d, PARTNER, &propose).unwrap();
    assert!(!declined.accepted && declined.objections().contains("Beziehung"), "{declined:?}");
    // A demand of a weak player is refused, explained by strength and pride.
    let demand = actions::judge(&state, d, PARTNER, &DiplomaticAction::DemandTribute { gold: 100 }).unwrap();
    assert!(!demand.accepted && demand.reasons.iter().any(|r| r.label == "Eure Stärke im Vergleich"), "{demand:?}");
    // The relation breakdown sums up to the relation.
    let parts = relations::relation_parts(&state, d, PARTNER);
    assert_eq!(parts.iter().map(|p| p.points).sum::<i32>(), relations::relation(&state, d, PARTNER));
    assert_eq!(relations::relation(&state, d, PARTNER), -40);
    assert_eq!(relations::relation_state(&state, d, PARTNER), RelationState::Rival);
}

#[test]
fn treaties_are_signed_expire_and_cost_when_broken() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40), IVec2::new(100, 100)]);
    let other = KingdomId(1);
    let cmd = |action| Command::Diplomacy { kingdom: PARTNER, action };
    // A friendly kingdom accepts a trade agreement.
    exec(&mut state, Command::DebugSetRelation { kingdom: PARTNER, value: 60 });
    let events = exec(&mut state, cmd(DiplomaticAction::ProposeTreaty { treaty: TreatyKind::TradeAgreement }));
    assert!(events.iter().any(|e| matches!(e, SimEvent::TreatySigned { kingdom: PARTNER, treaty: TreatyKind::TradeAgreement })), "{events:?}");
    let tick = state.clock.tick;
    let treaty = *state.diplomacy.kingdoms[0].treaty(TreatyKind::TradeAgreement, tick).unwrap();
    assert_eq!(treaty.until_tick - treaty.since_tick, 24 * TICKS_PER_DAY);
    // An alliance needs a pact first; a pact rules out war.
    let events = exec(&mut state, cmd(DiplomaticAction::ProposeTreaty { treaty: TreatyKind::Alliance }));
    assert!(matches!(rejection(&events), Some(RejectReason::DiplomacyUnavailable { reason }) if reason.contains("Nichtangriffspakt")));
    exec(&mut state, Command::DebugSetRelation { kingdom: PARTNER, value: 80 });
    exec(&mut state, cmd(DiplomaticAction::ProposeTreaty { treaty: TreatyKind::NonAggression }));
    assert!(state.diplomacy.kingdoms[0].has_treaty(TreatyKind::NonAggression, state.clock.tick));
    let events = exec(&mut state, cmd(DiplomaticAction::DeclareWar));
    assert!(matches!(rejection(&events), Some(RejectReason::DiplomacyUnavailable { reason }) if reason.contains("kündigt ihn zuerst")));
    // Cancelling: the partner remembers the broken treaty, the others the broken word.
    let before_other = relations::relation(&state, d, other);
    let events = exec(&mut state, cmd(DiplomaticAction::CancelTreaty { treaty: TreatyKind::NonAggression }));
    assert!(events.iter().any(|e| matches!(e, SimEvent::TreatyEnded { broken: true, .. })));
    assert!(state.diplomacy.kingdoms[0].memories.iter().any(|m| m.cause == MemoryCause::TreatyBroken && m.centi == -3000));
    assert_eq!(relations::relation(&state, d, other), before_other - 6);
    // The trade agreement runs out after 24 days.
    let events = run(&mut state, 24 * TICKS_PER_DAY);
    assert!(events.iter().any(|e| matches!(e, SimEvent::TreatyEnded { kingdom: PARTNER, treaty: TreatyKind::TradeAgreement, broken: false })));
    assert!(state.diplomacy.kingdoms[0].treaties.iter().all(|t| t.kind != TreatyKind::TradeAgreement));
    state.validate(d).expect("valid state");
}

#[test]
fn gifts_move_gold_and_goods_and_raise_the_relation() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let wood = res("wood");
    let (gold_before, wood_before) = (gold_everywhere(&state), goods_everywhere(&state, wood));
    let relation = relations::relation(&state, d, PARTNER);
    let treasury = state.economy.treasury();
    let gift = DiplomaticAction::Gift { gold: 200, goods: vec![ResourceAmount { resource: wood, amount: 50 }] };
    let events = exec(&mut state, Command::Diplomacy { kingdom: PARTNER, action: gift });
    assert!(events.iter().any(|e| matches!(e, SimEvent::GiftSent { gold: 200, .. })), "{events:?}");
    assert!(relations::relation(&state, d, PARTNER) > relation);
    assert!(state.economy.treasury() <= treasury - 200 + 1, "paid (hourly bookings aside)");
    assert_eq!(goods_everywhere(&state, wood), wood_before, "the wood moved to the kingdom");
    assert_eq!(state.economy.ledger().today().gold(crate::economy::LedgerCategory::Diplomacy), -200);
    let _ = gold_before;
    let too_much = DiplomaticAction::Gift { gold: 1_000_000, goods: Vec::new() };
    let events = exec(&mut state, Command::Diplomacy { kingdom: PARTNER, action: too_much });
    assert!(matches!(rejection(&events), Some(RejectReason::DiplomacyUnavailable { reason }) if reason.starts_with("Es fehlen")));
}

/// The fixture with standing orders the partner matches: stone above 100 for sale (the
/// partner, without stone of its own, wants it) and wool below 350 wanted (it has pastures).
fn trading_fixture() -> GameState {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let (stone, wool) = (res("stone"), res("wool"));
    state.diplomacy.kingdoms[0].stock_milli[stone.index()] = 0;
    state.diplomacy.kingdoms[0].next_caravan_tick = 0;
    relations::set_relation(&mut state, d, PARTNER, 20);
    for order in [
        TradeOrder { resource: stone, buy_below: None, sell_above: Some(100), daily_limit: 60 },
        TradeOrder { resource: wool, buy_below: Some(350), sell_above: None, daily_limit: 60 },
    ] {
        exec(&mut state, Command::SetTradeOrder { order });
    }
    assert!(economy::offered_units(d, &state.diplomacy.kingdoms[0], wool) > 0, "the partner offers wool");
    assert!(economy::wanted_units(d, &state.diplomacy.kingdoms[0], stone) > 0, "the partner wants stone");
    state
}

#[test]
fn foreign_caravans_walk_in_trade_by_the_orders_and_conserve_goods() {
    let d = defs();
    let mut state = trading_fixture();
    let (stone, wool) = (res("stone"), res("wool"));
    let stone_player = state.economy.stock().get(stone);
    let wool_player = state.economy.stock().get(wool);
    let mut sums = (goods_everywhere(&state, stone), goods_everywhere(&state, wool), gold_everywhere(&state));
    let mut traded = None;
    let mut walked = false;
    let mut saved = None;
    for _ in 0..2 * TICKS_PER_DAY {
        let events = state.tick(d, Vec::new());
        let now = (goods_everywhere(&state, stone), goods_everywhere(&state, wool), gold_everywhere(&state));
        let clock = state.clock;
        if !clock.is_new_day() {
            assert_eq!((now.0, now.1), (sums.0, sums.1), "goods conserved at tick {}", clock.tick);
        }
        if !clock.is_new_hour() {
            assert_eq!(now.2, sums.2, "gold conserved at tick {}", clock.tick);
        }
        sums = now;
        for e in &events {
            if let SimEvent::CaravanTraded { sold, bought, .. } = e {
                traded = Some((sold.clone(), bought.clone()));
            }
        }
        // Merchants and the cart walk on the map.
        if let Some(c) = state.diplomacy.caravans.first()
            && c.stage == CaravanStage::Arriving
            && c.walkers.len() == 2
        {
            walked = true;
            if saved.is_none() {
                saved = Some(state.clone());
            }
            let cart = state.population.walking.travelers.get(c.walkers[1]).unwrap();
            assert_eq!(cart.role(), crate::population::CitizenRole::Cart);
            let view = crate::population::walker_view(&state, d, crate::population::WalkerId::Traveler(c.walkers[1])).unwrap();
            assert_eq!(view.role.sprite_key(), "cart");
            assert!(view.carried.is_some(), "the cart shows its load");
        }
        if traded.is_some() && state.diplomacy.caravans.is_empty() {
            break;
        }
    }
    assert!(walked, "the caravan walked to the trading post");
    let (sold, bought) = traded.expect("the caravan traded");
    assert!(sold.iter().any(|m| m.resource == stone && m.amount > 0), "sold stone: {sold:?}");
    assert!(bought.iter().any(|m| m.resource == wool && m.amount > 0), "bought wool: {bought:?}");
    assert!(state.economy.stock().get(stone) < stone_player);
    assert!(state.economy.stock().get(wool) > wool_player);
    assert!(state.diplomacy.caravans.is_empty(), "the caravan went home");
    assert!(state.population.walking.travelers.is_empty(), "its walkers left the map");
    assert!(state.diplomacy.kingdoms[0].memories.iter().any(|m| m.cause == MemoryCause::Trade));
    state.validate(d).expect("valid state");
    // A game saved while the merchants walk continues identically.
    let saved = saved.expect("saved mid-caravan");
    let config = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&saved, config).unwrap();
    let (mut loaded, _): (GameState, _) = bincode::serde::decode_from_slice(&bytes, config).unwrap();
    loaded.validate(d).expect("valid loaded state");
    let mut original = saved;
    for _ in 0..TICKS_PER_DAY / 2 {
        assert_eq!(original.tick(d, Vec::new()), loaded.tick(d, Vec::new()));
    }
    assert_eq!(bincode::serde::encode_to_vec(&original, config).unwrap(), bincode::serde::encode_to_vec(&loaded, config).unwrap());
}

#[test]
fn own_caravans_sell_buy_and_return() {
    let d = defs();
    let mut state = trading_fixture();
    let (stone, wool) = (res("stone"), res("wool"));
    let wool_before = state.economy.stock().get(wool);
    let send = Command::SendCaravan {
        kingdom: PARTNER,
        goods: vec![ResourceAmount { resource: stone, amount: 20 }],
        gold: 100,
        buy: vec![ResourceAmount { resource: wool, amount: 10 }],
    };
    exec(&mut state, Command::RemoveTradeOrder { resource: stone });
    exec(&mut state, Command::RemoveTradeOrder { resource: wool });
    let events = exec(&mut state, send);
    assert!(events.iter().any(|e| matches!(e, SimEvent::CaravanDeparted { kind: CaravanKind::Own, .. })), "{events:?}");
    assert_eq!(state.diplomacy.caravans[0].carried(stone), 20);
    let mut sums = (goods_everywhere(&state, stone), goods_everywhere(&state, wool));
    let mut returned = None;
    for _ in 0..3 * TICKS_PER_DAY {
        let events = state.tick(d, Vec::new());
        let now = (goods_everywhere(&state, stone), goods_everywhere(&state, wool));
        if !state.clock.is_new_day() {
            assert_eq!(now, sums, "goods conserved at tick {}", state.clock.tick);
        }
        sums = now;
        if let Some(r) = events.iter().find_map(|e| match e {
            SimEvent::CaravanReturned { goods, gold, .. } => Some((goods.clone(), *gold)),
            _ => None,
        }) {
            returned = Some(r);
            break;
        }
    }
    let (goods, gold) = returned.expect("the caravan came back");
    assert!(goods.iter().any(|m| m.resource == wool && m.amount == 10), "{goods:?}");
    assert!(gold > 0, "it brought gold back");
    assert_eq!(state.economy.stock().get(wool), wool_before + 10);
    let exports = |l: &crate::economy::DayLedger| l.gold(crate::economy::LedgerCategory::Exports);
    let ledger = state.economy.ledger();
    assert!(exports(ledger.today()) > 0 || ledger.history().any(|day| exports(day) > 0), "the sale is booked as exports");
    state.validate(d).expect("valid state");
}

#[test]
fn war_blocks_trade_and_peace_restores_it() {
    let d = defs();
    let mut state = trading_fixture();
    let events = exec(&mut state, Command::Diplomacy { kingdom: PARTNER, action: DiplomaticAction::DeclareWar });
    assert!(events.iter().any(|e| matches!(e, SimEvent::WarDeclared { attacker: Party::Player, .. })));
    assert!(!has_access(&state, d, PARTNER));
    assert!(war_weariness(&state, d) < 0, "the citizens feel the war");
    let send = Command::SendCaravan { kingdom: PARTNER, goods: vec![ResourceAmount { resource: res("stone"), amount: 5 }], gold: 0, buy: Vec::new() };
    let events = exec(&mut state, send);
    assert!(matches!(rejection(&events), Some(RejectReason::CaravanUnavailable { reason }) if reason.contains("Krieg")));
    let events = run(&mut state, 2 * TICKS_PER_DAY);
    assert!(!events.iter().any(|e| matches!(e, SimEvent::CaravanDeparted { .. })), "no caravans during the war");
    // Exhausted, the kingdom accepts a white peace.
    if let Some(w) = &mut state.diplomacy.kingdoms[0].war {
        w.exhaustion = 200;
    }
    let peace = Command::Diplomacy { kingdom: PARTNER, action: DiplomaticAction::ProposePeace { terms: PeaceTerms::White } };
    let events = exec(&mut state, peace);
    assert!(events.iter().any(|e| matches!(e, SimEvent::PeaceMade { kingdom: PARTNER, .. })), "{events:?}");
    exec(&mut state, Command::DebugSetRelation { kingdom: PARTNER, value: 10 });
    assert!(has_access(&state, d, PARTNER));
    state.validate(d).expect("valid state");
}

#[test]
fn raids_take_unprotected_goods_to_the_enemy() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let bread = res("bread");
    let actions_war = Command::Diplomacy { kingdom: PARTNER, action: DiplomaticAction::DeclareWar };
    exec(&mut state, actions_war);
    let before = goods_everywhere(&state, bread);
    let mut raided = Vec::new();
    for _ in 0..4 * TICKS_PER_DAY {
        for e in state.tick(d, Vec::new()) {
            if let SimEvent::RaidHappened { lost, .. } = e {
                raided.extend(lost);
            }
        }
    }
    let taken: u32 = raided.iter().filter(|m| m.resource == bread).map(|m| m.amount).sum();
    assert!(taken > 0, "the raid took bread: {raided:?}");
    let _ = before;
    assert!(state.diplomacy.kingdoms[0].war.is_some_and(|w| w.raids >= 1));
    state.validate(d).expect("valid state");
}

#[test]
fn caravans_on_dangerous_routes_are_robbed_by_the_enemy() {
    let defs_robbery = crate::test_support::defs_edited("rules.ron", |s| {
        s.replace("robbery_war_percent: 25", "robbery_war_percent: 100").replace("robbery_max_percent: 50", "robbery_max_percent: 100")
    });
    let mut state = kingdom_fixture(&[IVec2::new(104, 40), IVec2::new(64, 32)]);
    let robber = KingdomId(1);
    let d = &defs_robbery;
    relations::set_relation(&mut state, d, PARTNER, 20);
    state.tick(d, vec![Command::Diplomacy { kingdom: robber, action: DiplomaticAction::DeclareWar }]);
    assert_eq!(caravans::route_danger(&state, d, PARTNER), (100, Some(robber)));
    let stone = res("stone");
    let robber_stone = state.diplomacy.kingdoms[1].stock(stone);
    let send = Command::SendCaravan { kingdom: PARTNER, goods: vec![ResourceAmount { resource: stone, amount: 15 }], gold: 0, buy: Vec::new() };
    state.tick(d, vec![send]);
    let mut robbed = None;
    for _ in 0..2 * TICKS_PER_DAY {
        for e in state.tick(d, Vec::new()) {
            if let SimEvent::CaravanRobbed { robber: who, goods, .. } = e {
                robbed = Some((who, goods));
            }
        }
        if robbed.is_some() {
            break;
        }
    }
    let (who, goods) = robbed.expect("robbed on the way");
    assert_eq!(who, robber);
    assert_eq!(goods, vec![ResourceAmount { resource: stone, amount: 15 }]);
    assert!(state.diplomacy.kingdoms[1].stock(stone) >= robber_stone + 15 - 1, "the robbers keep the stone");
}

#[test]
fn kingdoms_take_initiatives_and_respect_the_grace_period() {
    let d = defs();
    // Nobody plays: without defeat the settlement may starve while the kingdoms act.
    let mut state = GameState::new(d, GameSettings::new(d, 7, WorldSize::Small).with_events(false).with_defeat(false));
    let ai = d.rules().diplomacy.ai;
    let mut proposals = Vec::new();
    // Tick of every paced proposal (everything but peace offers).
    let mut paced: Vec<u64> = Vec::new();
    let mut war_day = None;
    for _ in 0..40 * TICKS_PER_DAY {
        for e in state.tick(d, Vec::new()) {
            match e {
                SimEvent::ProposalReceived { serial, kingdom, .. } => {
                    proposals.push((serial, kingdom));
                    let peace = state.diplomacy.proposals().iter().any(|p| p.serial == serial && matches!(p.kind, ProposalKind::Peace { .. }));
                    if !peace {
                        paced.push(state.clock.tick);
                    }
                }
                SimEvent::WarDeclared { defender: Party::Player, .. } => {
                    war_day.get_or_insert(state.clock.total_days());
                }
                _ => {}
            }
        }
        // The inbox stays small: one proposal per kingdom, a few in all.
        let open = state.diplomacy.proposals();
        assert!(open.len() <= usize::from(ai.max_open_proposals) + state.diplomacy.kingdoms().filter(|(_, k)| k.at_war()).count());
        let senders: std::collections::BTreeSet<KingdomId> = open.iter().map(|p| p.kingdom).collect();
        assert_eq!(senders.len(), open.len(), "one open proposal per kingdom");
    }
    assert!(proposals.len() >= 5, "kingdoms act on their own: {proposals:?}");
    // Paced: none in the first days, then at least a day apart (two in the first days).
    assert!(paced.iter().all(|t| *t / TICKS_PER_DAY >= u64::from(ai.first_proposal_day)), "no proposal in the first days: {paced:?}");
    for pair in paced.windows(2) {
        let gap = if pair[1] / TICKS_PER_DAY < u64::from(ai.early_days) { ai.early_proposal_gap_hours } else { ai.proposal_gap_hours };
        assert!(pair[1] - pair[0] >= u64::from(gap) * TICKS_PER_HOUR, "proposals too close together: {paced:?}");
    }
    let senders: std::collections::BTreeSet<KingdomId> = proposals.iter().map(|(_, k)| *k).collect();
    assert!(senders.len() >= 2, "several kingdoms propose");
    if let Some(day) = war_day {
        assert!(day >= u64::from(d.rules().diplomacy.ai.war_grace_days), "no war before the grace period ends (day {day})");
    }
    assert!(state.diplomacy.history().len() >= 5);
    assert!(state.diplomacy.proposals().iter().all(|p| p.expires_tick > state.clock.tick), "unanswered proposals expire");
    state.validate(d).expect("valid state");
}

#[test]
fn answers_to_proposals_apply_their_terms() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let wool = res("wool");
    // A sale offer: the player buys wool for gold.
    let serial = state.diplomacy.next_serial;
    state.diplomacy.next_serial += 1;
    let tick = state.clock.tick;
    state.diplomacy.proposals.push(Proposal {
        serial,
        kingdom: PARTNER,
        kind: ProposalKind::Sale { resource: wool, amount: 10, gold: 50 },
        created_tick: tick,
        expires_tick: tick + 10 * TICKS_PER_HOUR,
    });
    let wool_before = state.economy.stock().get(wool);
    let kingdom_wool = state.diplomacy.kingdoms[0].stock(wool);
    let events = exec(&mut state, Command::AnswerProposal { serial, accept: true });
    assert!(events.iter().any(|e| matches!(e, SimEvent::ProposalAnswered { accepted: true, expired: false, .. })), "{events:?}");
    assert_eq!(state.economy.stock().get(wool), wool_before + 10);
    assert_eq!(state.diplomacy.kingdoms[0].stock(wool), kingdom_wool - 10);
    let events = exec(&mut state, Command::AnswerProposal { serial, accept: true });
    assert_eq!(rejection(&events), Some(&RejectReason::NoProposal));
    // A purchase offer is paid from the kingdom's treasury as it is when the player accepts: a
    // kingdom that spent its gold meanwhile buys only what it can still pay for.
    let purchase = |state: &mut GameState, treasury_gold: i64| {
        state.diplomacy.kingdoms[0].treasury_milli = treasury_gold * crate::economy::MILLI_PER_GOLD;
        let serial = state.diplomacy.next_serial;
        state.diplomacy.next_serial += 1;
        let tick = state.clock.tick;
        state.diplomacy.proposals.push(Proposal {
            serial,
            kingdom: PARTNER,
            kind: ProposalKind::Purchase { resource: wool, amount: 10, gold: 100 },
            created_tick: tick,
            expires_tick: tick + 10 * TICKS_PER_HOUR,
        });
        let wool_before = state.economy.stock().get(wool);
        let events = exec(state, Command::AnswerProposal { serial, accept: true });
        (events, wool_before - state.economy.stock().get(wool), serial)
    };
    let (events, sold, _) = purchase(&mut state, 45);
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    assert_eq!(sold, 4, "45 gold pay for 4 of the 10 units at 10 gold each");
    assert_eq!(state.diplomacy.kingdoms[0].treasury_milli, 5 * crate::economy::MILLI_PER_GOLD, "the treasury never goes below zero");
    let (events, sold, serial) = purchase(&mut state, 0);
    assert!(matches!(rejection(&events), Some(RejectReason::DiplomacyUnavailable { reason }) if reason.contains("nicht mehr bezahlen")));
    assert_eq!(sold, 0);
    assert!(state.diplomacy.proposals.iter().any(|p| p.serial == serial), "the offer can still be declined");
    // A demand that runs out counts as refused.
    let serial = state.diplomacy.next_serial;
    state.diplomacy.next_serial += 1;
    let tick = state.clock.tick;
    state.diplomacy.proposals.push(Proposal { serial, kingdom: PARTNER, kind: ProposalKind::Tribute { gold: 60 }, created_tick: tick, expires_tick: tick + 2 });
    let events = run(&mut state, 3);
    assert!(events.iter().any(|e| matches!(e, SimEvent::ProposalAnswered { accepted: false, expired: true, .. })));
    assert!(state.diplomacy.kingdoms[0].memories.iter().any(|m| m.cause == MemoryCause::DemandRefused));
    state.validate(d).expect("valid state");
}

#[test]
fn developer_commands_grant_spawn_and_set_relations() {
    let d = defs();
    let mut state = kingdom_fixture(&[IVec2::new(100, 40)]);
    let treasury = state.economy.treasury();
    let events = exec(&mut state, Command::DebugGrant { gold: 1000, goods: vec![ResourceAmount { resource: res("spices"), amount: 30 }] });
    assert!(events.iter().any(|e| matches!(e, SimEvent::DebugGranted { gold: 1000, .. })));
    assert!(state.economy.treasury() >= treasury + 999);
    let people = state.population.len();
    let events = exec(&mut state, Command::DebugSpawnHouseholds { count: 4 });
    assert_eq!(events.iter().filter(|e| matches!(e, SimEvent::HouseholdArrived { .. })).count(), 4);
    assert_eq!(state.population.len(), people + 3 + 2 + 4 + 1);
    assert_eq!(rejection(&exec(&mut state, Command::DebugSpawnHouseholds { count: 0 })), Some(&RejectReason::InvalidHouseholdCount));
    exec(&mut state, Command::DebugSetRelation { kingdom: PARTNER, value: -70 });
    assert_eq!(relations::relation(&state, d, PARTNER), -70);
    assert_eq!(relations::relation_state(&state, d, PARTNER), RelationState::Hostile);
    let bad = exec(&mut state, Command::DebugSetRelation { kingdom: KingdomId(5), value: 0 });
    assert_eq!(rejection(&bad), Some(&RejectReason::UnknownKingdom));
    state.validate(d).expect("valid state");
}

#[test]
fn validation_catches_broken_diplomacy_data() {
    let d = defs();
    let state = kingdom_fixture(&[IVec2::new(100, 40)]);
    state.validate(d).expect("valid");
    let mut broken = state.clone();
    broken.diplomacy.kingdoms[0].territory_tiles += 1;
    assert!(matches!(broken.validate(d), Err(crate::state::StateError::InvalidDiplomacy(_))));
    let mut broken = state.clone();
    let s = broken.diplomacy.kingdoms[0].structures[0];
    broken.diplomacy.land.set_structure(s.rect(d), false);
    assert!(matches!(broken.validate(d), Err(crate::state::StateError::InvalidDiplomacy(_))));
    let mut broken = state.clone();
    broken.diplomacy.ai_relations = vec![5];
    assert!(matches!(broken.validate(d), Err(crate::state::StateError::InvalidDiplomacy(_))));
    let mut broken = state;
    broken.diplomacy.kingdoms[0].war = Some(War { since_tick: 0, declared_by_player: true, exhaustion: 0, raids: 0, next_raid_day: 0 });
    broken.diplomacy.kingdoms[0].treaties.push(Treaty { kind: TreatyKind::TradeAgreement, since_tick: 0, until_tick: 10 });
    assert!(matches!(broken.validate(d), Err(crate::state::StateError::InvalidDiplomacy(_))));
}

#[test]
fn reports_describe_kingdoms_actions_inbox_and_caravans() {
    let d = defs();
    let mut state = trading_fixture();
    let report = crate::report::diplomacy_report(&state, d);
    assert_eq!(report.kingdoms.len(), 1);
    let row = &report.kingdoms[0];
    assert_eq!(row.relation, 20);
    assert_eq!(row.state_name, "Neutral");
    assert!(row.access && row.access_text == "Handel möglich.");
    assert!(row.wanted.iter().any(|g| g.name == "Stein" && g.price_centi > 0), "{:?}", row.wanted);
    assert!(row.ruler.starts_with(&state.diplomacy.kingdoms[0].title));
    assert!(!row.strength_text.is_empty() && row.structures > 0);
    let actions = crate::report::kingdom_actions(&state, d, PARTNER);
    assert_eq!(actions.len(), 6, "three treaties, gift, demand, war");
    let trade = &actions[0];
    assert_eq!(trade.label, "Handelsvertrag vorschlagen");
    assert!(trade.problem.is_none() && trade.prospect.is_some());
    assert!(actions.iter().any(|a| a.label == "Bündnis vorschlagen" && a.problem.as_deref() == Some("Ein Bündnis setzt einen Nichtangriffspakt voraus.")));
    // Prices with the partner for every good.
    let prices = crate::report::partner_prices(&state, d, PARTNER);
    assert_eq!(prices.len(), d.resources().len());
    assert!(prices.iter().all(|p| p.buy_centi >= p.sell_centi), "buying never costs less than selling earns");
    // Even with the best terms (alliance, top relation, every trade research): the bonus
    // narrows the margin to nothing but never inverts it.
    {
        let mut best = state.clone();
        exec(&mut best, Command::DebugUnlockAllResearch);
        exec(&mut best, Command::DebugSetRelation { kingdom: PARTNER, value: 100 });
        let tick = best.clock.tick;
        best.diplomacy.kingdoms[0].treaties.push(Treaty { kind: TreatyKind::Alliance, since_tick: tick, until_tick: tick + TICKS_PER_DAY });
        assert!(crate::diplomacy::economy::price_bonus_percent(&best, d, PARTNER) > i32::from(d.rules().diplomacy.caravans.margin_percent));
        let prices = crate::report::partner_prices(&best, d, PARTNER);
        assert!(prices.iter().all(|p| p.buy_centi >= p.sell_centi), "{prices:?}");
        assert!(prices.iter().any(|p| p.buy_centi == p.sell_centi && p.buy_centi > 1));
    }
    // A caravan on its way and a proposal in the inbox.
    run(&mut state, 2 * TICKS_PER_HOUR);
    let caravans = crate::report::caravans_report(&state, d);
    assert!(!caravans.is_empty() && caravans[0].stage_text.starts_with("Unterwegs aus"), "{caravans:?}");
    let serial = state.diplomacy.next_serial;
    state.diplomacy.next_serial += 1;
    let tick = state.clock.tick;
    state.diplomacy.proposals.push(Proposal {
        serial,
        kingdom: PARTNER,
        kind: ProposalKind::Treaty(TreatyKind::NonAggression),
        created_tick: tick,
        expires_tick: tick + 3 * TICKS_PER_HOUR,
    });
    let inbox = crate::report::pending_proposals(&state, d);
    assert_eq!(inbox[0].title, "Nichtangriffspakt angeboten");
    assert_eq!(inbox[0].remaining_text, "noch 3 h");
    // At war the actions offer peace.
    exec(&mut state, Command::Diplomacy { kingdom: PARTNER, action: DiplomaticAction::DeclareWar });
    let actions = crate::report::kingdom_actions(&state, d, PARTNER);
    assert!(actions.iter().filter(|a| matches!(a.action, DiplomaticAction::ProposePeace { .. })).count() == 4);
    assert_eq!(crate::report::diplomacy_report(&state, d).kingdoms[0].state_name, "Im Krieg");
}

#[test]
#[ignore = "performance measurement; run in release mode"]
fn diplomacy_performance() {
    let d = defs();
    for size in [WorldSize::Medium, WorldSize::Huge] {
        let mut state = GameState::new(d, GameSettings::new(d, 7, size).with_events(false));
        exec(&mut state, Command::DebugUnlockAllResearch);
        exec(&mut state, Command::DebugGrant { gold: 5000, goods: vec![ResourceAmount { resource: res("planks"), amount: 50 }] });
        let hq = storage::main_headquarters(&state, d).expect("headquarters");
        let kind = d.building_id("trading_post").unwrap();
        let near = state.building(hq).unwrap().entrance();
        let origin = crate::start::spiral(20)
            .map(|o| near + o + IVec2::new(4, 4))
            .find(|o| crate::buildings::check_placement(&state, d, kind, *o, crate::grid::Rotation::default()).ok)
            .expect("spot for the trading post");
        let post = crate::buildings::spawn_finished(&mut state, d, kind, origin, crate::grid::Rotation::default()).unwrap();
        crate::test_support::link(&mut state, post, hq);
        exec(&mut state, Command::DebugGrant { gold: 0, goods: vec![ResourceAmount { resource: res("stone"), amount: 200 }] });
        for (key, buy, sell) in [("stone", None, Some(20)), ("bread", Some(200), None), ("wool", Some(100), None), ("spices", Some(40), None)] {
            exec(&mut state, Command::SetTradeOrder { order: TradeOrder { resource: res(key), buy_below: buy, sell_above: sell, daily_limit: 50 } });
        }
        for i in 0..state.diplomacy.len() {
            let id = KingdomId(i as u8);
            exec(&mut state, Command::DebugSetRelation { kingdom: id, value: 40 });
            state.diplomacy.kingdoms[i].next_caravan_tick = 0;
        }
        let ticks = 4 * TICKS_PER_DAY;
        let (mut total, mut max, mut tick_total, mut caravans, mut walkers) = (0u128, 0u128, 0u128, 0usize, 0usize);
        let mut departed = 0;
        for _ in 0..ticks {
            let before = state.diplomacy.caravans.len();
            let timed = state.tick_timed(d);
            let ns = timed.iter().find(|(n, _)| *n == "diplomacy").map_or(0, |(_, ns)| *ns);
            total += ns;
            max = max.max(ns);
            tick_total += timed.iter().map(|(_, ns)| *ns).sum::<u128>();
            caravans = caravans.max(state.diplomacy.caravans.len());
            walkers = walkers.max(state.population.walking.travelers.len());
            departed += state.diplomacy.caravans.len().saturating_sub(before);
        }
        eprintln!(
            "{size:?}: {} kingdoms, diplomacy Ø {:.2} µs/tick, max {:.0} µs; whole tick Ø {:.1} µs; caravans at once ≤ {caravans}, walkers ≤ {walkers}, new caravans {departed}",
            state.diplomacy.len(),
            total as f64 / ticks as f64 / 1000.0,
            max as f64 / 1000.0,
            tick_total as f64 / ticks as f64 / 1000.0,
        );
        assert!(departed > 0, "caravans were active");
        state.validate(d).expect("valid state");
    }
}
