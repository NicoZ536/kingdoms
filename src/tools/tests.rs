use std::sync::Arc;

use glam::IVec2;
use kingdoms_sim::buildings::{BuildingId, Rotation, check_placement, demolish_refund};
use kingdoms_sim::command::{Command, RejectReason};
use kingdoms_sim::defs::{BuildingRole, Defs};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::state::GameSettings;
use kingdoms_sim::world::WorldSize;

use super::*;
use crate::session::Session;

fn defs() -> Arc<Defs> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    match Defs::load(&dir) {
        Ok(defs) => Arc::new(defs),
        Err(e) => panic!("shipped game data must load: {e}"),
    }
}

fn session() -> Session {
    let defs = defs();
    Session::new(defs.clone(), GameSettings::new(&defs, 7, WorldSize::Small))
}

fn hover(tile: IVec2) -> ToolInput {
    ToolInput { tile: Some(tile), ..Default::default() }
}

/// Feeds a full click (press, then release without drag) on `tile`.
fn click(tools: &mut ToolController, session: &Session, tile: IVec2, building: Option<BuildingId>) -> ToolOutput {
    let base = ToolInput { tile: Some(tile), building, ..Default::default() };
    let pressed = tools.update(session, &ToolInput { pressed: true, down: true, ..base });
    assert!(pressed.commands.is_empty());
    tools.update(session, &ToolInput { released: true, ..base })
}

/// A cursor tile where a `key` building fits (rotation 0), searched around the headquarters.
fn free_spot(session: &Session, key: &str) -> IVec2 {
    let defs = session.defs();
    let kind = defs.building_id(key).expect("building type");
    let size = defs.building(kind).size;
    let home = session.home_tile();
    for r in 3..30 {
        for y in -r..=r {
            for x in -r..=r {
                let tile = home + IVec2::new(x, y);
                let check = check_placement(session.state(), defs, kind, centred_origin(tile, size), Rotation::default());
                if check.ok {
                    return tile;
                }
            }
        }
    }
    panic!("no free spot for {key}");
}

fn headquarters(session: &Session) -> (BuildingId, IVec2) {
    let id = session.headquarters().expect("headquarters");
    let origin = session.state().building(id).expect("building").origin();
    (id, origin)
}

#[test]
fn geometry_helpers() {
    assert_eq!(centred_origin(IVec2::new(10, 10), IVec2::new(2, 3)), IVec2::new(10, 9));
    assert_eq!(centred_origin(IVec2::new(10, 10), IVec2::new(1, 1)), IVec2::new(10, 10));
    assert_eq!(centred_origin(IVec2::new(10, 10), IVec2::new(4, 4)), IVec2::new(9, 9));
    assert_eq!(l_line(IVec2::new(1, 1), IVec2::new(3, 0)), vec![IVec2::new(1, 1), IVec2::new(2, 1), IVec2::new(3, 1), IVec2::new(3, 0)]);
    assert_eq!(l_line(IVec2::new(2, 2), IVec2::new(2, 2)), vec![IVec2::new(2, 2)]);
}

#[test]
fn build_tool_places_through_a_command_and_returns_to_inspect() {
    let mut session = session();
    let kind = session.defs().building_id("woodcutter").expect("woodcutter");
    let tile = free_spot(&session, "woodcutter");
    let mut tools = ToolController::new();
    assert!(tools.select(Tool::Build { kind }));
    let out = tools.update(&session, &hover(tile));
    assert!(out.commands.is_empty());
    let preview = tools.placement().expect("preview");
    assert!(preview.check.ok, "{:?}", preview.check.errors);
    assert_eq!(preview.check.origin, centred_origin(tile, IVec2::new(2, 2)));

    let access = preview.access.as_ref().filter(|p| p.valid && p.new_tile_count() > 0).map(|p| p.road);
    let out = click(&mut tools, &session, tile, None);
    let origin = centred_origin(tile, IVec2::new(2, 2));
    let rotation = Rotation::default();
    // A suggested access road is laid by the simulation together with the building.
    let expected = match access {
        Some(road) => Command::PlaceBuildingWithAccess { kind, origin, rotation, road },
        None => Command::PlaceBuilding { kind, origin, rotation },
    };
    assert_eq!(out.commands, vec![expected]);
    assert_eq!(tools.tool(), Tool::Inspect);

    for command in out.commands {
        session.queue_command(command);
    }
    let mut cursor = session.events().cursor_at_end();
    session.tick();
    let events: Vec<_> = session.events().read(&mut cursor).events.map(|e| e.event.clone()).collect();
    assert!(events.iter().any(|e| matches!(e, SimEvent::BuildingPlaced { kind: k, .. } if *k == kind)), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, SimEvent::CommandRejected { .. })), "{events:?}");
}

#[test]
fn shift_keeps_building_and_invalid_spots_stay_in_the_tool() {
    let session = session();
    let kind = session.defs().building_id("woodcutter").expect("woodcutter");
    let mut tools = ToolController::new();
    tools.select(Tool::Build { kind });
    let tile = free_spot(&session, "woodcutter");
    let base = ToolInput { tile: Some(tile), shift: true, ..Default::default() };
    tools.update(&session, &ToolInput { pressed: true, down: true, ..base });
    let out = tools.update(&session, &ToolInput { released: true, ..base });
    assert!(matches!(out.commands.first(), Some(Command::PlaceBuilding { .. } | Command::PlaceBuildingWithAccess { .. })));
    assert_eq!(tools.tool(), Tool::Build { kind });

    // On the headquarters the check fails: the command still goes to the simulation (which
    // answers with the German reason), but the tool stays active.
    let (_, hq) = headquarters(&session);
    tools.update(&session, &hover(hq));
    assert!(!tools.placement().expect("preview").check.ok);
    let out = click(&mut tools, &session, hq, None);
    assert_eq!(out.commands.len(), 1);
    assert_eq!(tools.tool(), Tool::Build { kind });
}

#[test]
fn rotation_turns_the_footprint() {
    let session = session();
    let kind = session.defs().building_id("sawmill").expect("sawmill");
    let mut tools = ToolController::new();
    assert!(!tools.rotate(1), "inspect does not rotate");
    tools.select(Tool::Build { kind });
    let tile = session.home_tile() + IVec2::new(12, 0);
    let out = tools.update(&session, &ToolInput { rotate: 1, ..hover(tile) });
    assert_eq!(out.sounds, vec!["ui_rotate"]);
    assert_eq!(tools.rotation(), Rotation::new(1).expect("rotation"));
    let footprint = tools.placement().expect("preview").check.footprint;
    assert_eq!(footprint.size, IVec2::new(3, 2));
    assert_eq!(footprint.origin, centred_origin(tile, IVec2::new(3, 2)));
    tools.update(&session, &ToolInput { rotate: -2, ..hover(tile) });
    assert_eq!(tools.rotation(), Rotation::new(3).expect("rotation"));
}

#[test]
fn road_drag_plans_and_builds_on_release() {
    let session = session();
    let road = session.defs().road_id("path").expect("path");
    let mut tools = ToolController::new();
    tools.select(Tool::Road { road });
    let start = free_spot(&session, "well");
    let end = start + IVec2::new(4, 0);
    tools.update(&session, &hover(start));
    assert_eq!(tools.road_hover().map(|p| p.positions()), Some(vec![start]));

    tools.update(&session, &ToolInput { pressed: true, down: true, ..hover(start) });
    assert_eq!(tools.road_drag().map(|d| d.plan.positions()), Some(vec![start]));
    tools.update(&session, &ToolInput { down: true, ..hover(start + IVec2::new(2, 0)) });
    tools.update(&session, &ToolInput { down: true, ..hover(end) });
    let drag = tools.road_drag().expect("drag");
    assert_eq!((drag.start, drag.end), (start, end));
    assert_eq!(drag.plan.tiles.first().map(|t| t.pos), Some(start));
    assert_eq!(drag.plan.tiles.last().map(|t| t.pos), Some(end));
    let tiles = drag.plan.positions();

    let out = tools.update(&session, &ToolInput { released: true, dragged: true, ..hover(end) });
    assert_eq!(out.commands, vec![Command::BuildRoad { tiles, road }]);
    assert!(tools.road_drag().is_none());
    assert_eq!(tools.tool(), Tool::Road { road }, "the road tool stays active");

    // Released over the UI: nothing is built.
    tools.update(&session, &ToolInput { pressed: true, down: true, ..hover(start) });
    let out = tools.update(&session, &ToolInput { released: true, dragged: true, over_ui: true, ..Default::default() });
    assert!(out.commands.is_empty());
}

#[test]
fn cancel_steps_back_from_drag_to_tool_to_inspect() {
    let session = session();
    let road = session.defs().road_id("path").expect("path");
    let mut tools = ToolController::new();
    assert!(!tools.cancel());
    tools.select(Tool::Road { road });
    let tile = session.home_tile() + IVec2::new(8, 8);
    tools.update(&session, &ToolInput { pressed: true, down: true, ..hover(tile) });
    assert!(tools.road_drag().is_some());
    let out = tools.update(&session, &ToolInput { secondary_click: true, down: true, ..hover(tile) });
    assert_eq!(out.sounds, vec!["ui_close"]);
    assert!(tools.road_drag().is_none());
    assert_eq!(tools.tool(), Tool::Road { road });
    // Releasing the button afterwards does not build anything.
    let out = tools.update(&session, &ToolInput { released: true, ..hover(tile) });
    assert!(out.commands.is_empty());
    assert!(tools.cancel());
    assert_eq!(tools.tool(), Tool::Inspect);
}

#[test]
fn inspect_click_selects() {
    let session = session();
    let (hq, origin) = headquarters(&session);
    let mut tools = ToolController::new();
    let out = click(&mut tools, &session, origin, Some(hq));
    assert_eq!(out.selection, Some(Some(Selection::Building(hq))));
    assert_eq!(out.sounds, vec!["ui_select"]);
    let tile = origin + IVec2::new(-6, -6);
    let out = click(&mut tools, &session, tile, None);
    assert_eq!(out.selection, Some(Some(Selection::Tile(tile))));
    // A drag (camera) is no click.
    tools.update(&session, &ToolInput { pressed: true, down: true, ..hover(tile) });
    let out = tools.update(&session, &ToolInput { released: true, dragged: true, ..hover(tile) });
    assert_eq!(out.selection, None);
}

#[test]
fn inspect_click_on_a_walker_opens_the_citizen_window() {
    let session = session();
    let (hq, origin) = headquarters(&session);
    let citizen = session.state().population().citizens().next().map(|(id, _)| id);
    let Some(citizen) = citizen else { panic!("the start has settlers") };
    let mut tools = ToolController::new();
    let base = ToolInput { tile: Some(origin), building: Some(hq), citizen: Some(citizen), ..Default::default() };
    tools.update(&session, &ToolInput { pressed: true, down: true, ..base });
    let out = tools.update(&session, &ToolInput { released: true, ..base });
    assert_eq!(out.inspect_citizen, Some(citizen));
    // The building behind the walker is not selected.
    assert_eq!(out.selection, None);
    assert_eq!(out.sounds, vec!["ui_select"]);
    // Other tools ignore walkers.
    tools.select(Tool::Demolish);
    tools.update(&session, &ToolInput { pressed: true, down: true, ..base });
    let out = tools.update(&session, &ToolInput { released: true, ..base });
    assert_eq!(out.inspect_citizen, None);
}

#[test]
fn demolish_confirms_the_headquarters_and_removes_roads() {
    let session = session();
    let (hq, origin) = headquarters(&session);
    let mut tools = ToolController::new();
    tools.select(Tool::Demolish);
    let out = click(&mut tools, &session, origin, Some(hq));
    assert_eq!(out.confirm, Some(DemolishRequest { building: hq }));
    assert!(out.commands.is_empty());

    let road_tile = session.state().roads().iter().map(|(p, _)| p).next().expect("start roads");
    let out = click(&mut tools, &session, road_tile, None);
    assert_eq!(out.commands, vec![Command::RemoveRoad { tiles: vec![road_tile] }]);

    // Dragging removes the roads along the line, nothing else.
    let roads: Vec<IVec2> = session.state().roads().iter().map(|(p, _)| p).collect();
    let (a, b) = roads
        .iter()
        .flat_map(|a| roads.iter().map(move |b| (*a, *b)))
        .find(|(a, b)| a.y == b.y && b.x - a.x >= 2 && l_line(*a, *b).iter().all(|t| roads.contains(t)))
        .expect("a straight road");
    tools.update(&session, &ToolInput { pressed: true, down: true, ..hover(a) });
    tools.update(&session, &ToolInput { down: true, ..hover(b) });
    let out = tools.update(&session, &ToolInput { released: true, dragged: true, ..hover(b) });
    assert_eq!(out.commands, vec![Command::RemoveRoad { tiles: l_line(a, b) }]);

    let out = click(&mut tools, &session, origin + IVec2::new(-9, -9), None);
    assert!(out.commands.is_empty());
    assert_eq!(out.sounds, vec!["ui_error"]);
}

#[test]
fn move_tool_refuses_the_headquarters_and_moves_other_buildings() {
    let session = session();
    let defs = session.defs();
    let (hq, origin) = headquarters(&session);
    let mut tools = ToolController::new();
    tools.select(Tool::Move { building: None });
    let out = click(&mut tools, &session, origin, Some(hq));
    assert_eq!(out.message.as_deref(), Some("Das Hauptquartier kann nicht versetzt werden."));
    assert_eq!(tools.tool(), Tool::Move { building: None });

    let (house, b) = session
        .state()
        .buildings()
        .iter()
        .find(|(_, b)| defs.building(b.kind()).role == BuildingRole::General && b.is_complete())
        .expect("a finished start building");
    let kind = b.kind();
    let out = click(&mut tools, &session, b.origin(), Some(house));
    assert_eq!(out.sounds, vec!["ui_select"]);
    assert_eq!(tools.tool(), Tool::Move { building: Some(house) });

    let key = defs.building(kind).key.clone();
    let target = free_spot(&session, &key);
    tools.update(&session, &hover(target));
    let preview = tools.placement().expect("move preview");
    assert_eq!(preview.moving, Some(house));
    let rotation = tools.rotation();
    let size = rotation.footprint_size(defs.building(kind).size);
    let out = click(&mut tools, &session, target, None);
    assert_eq!(
        out.commands,
        vec![Command::MoveBuilding { building: house, origin: centred_origin(target, size), rotation }]
    );
    if out.selection.is_some() {
        assert_eq!(tools.tool(), Tool::Inspect);
    }
}

#[test]
fn refunds_and_confirmation_rules() {
    let session = session();
    let defs = session.defs();
    let state = session.state();
    let (hq, _) = headquarters(&session);
    assert!(needs_confirmation(state, defs, hq));
    // The previews come from the simulation: the only headquarters can neither be demolished
    // nor moved, and the UI shows the simulation's reason.
    assert_eq!(demolish_refund(state, defs, hq), Err(RejectReason::LastHeadquarters));
    assert_eq!(movable(state, defs, hq), Err(RejectReason::CannotMoveHeadquarters.to_string()));
    // An inhabited start house asks first and returns half of its materials.
    let house = state
        .buildings()
        .iter()
        .find(|(id, b)| defs.building(b.kind()).housing.is_some() && state.population().resident_count(*id) > 0)
        .map(|(id, _)| id)
        .expect("inhabited start house");
    assert!(needs_confirmation(state, defs, house));
    let refund = demolish_refund(state, defs, house).expect("refund");
    assert!(!refund.cancels_site);
    let def = defs.building(state.building(house).expect("house").kind());
    for m in &def.cost.materials {
        let expected = m.amount / 2;
        if expected > 0 {
            assert!(refund.materials.iter().any(|r| r.resource == m.resource && r.amount >= expected));
        }
    }
    assert!(access_road_type(state, defs).is_some());
}

#[test]
fn access_suggestion_follows_the_placement_verdict() {
    let mut session = session();
    let defs = session.shared_defs();
    let kind = defs.building_id("woodcutter").expect("woodcutter");
    let tile = free_spot(&session, "woodcutter");
    let origin = centred_origin(tile, IVec2::new(2, 2));
    let mut tools = ToolController::new();
    tools.select(Tool::Build { kind });
    // The spot is taken by a site: invalid preview, no access planned.
    session.queue_command(Command::PlaceBuilding { kind, origin, rotation: Rotation::default() });
    session.tick();
    tools.update(&session, &hover(tile));
    let preview = tools.placement().expect("preview");
    assert!(!preview.check.ok);
    assert!(preview.access.is_none() && !preview.access_planned);
    // The site is cancelled: once the check runs again (well within the replanning interval)
    // the spot is valid and the access road is planned at once instead of reusing "none".
    let site = session.state().buildings().id_at(origin).expect("site");
    session.queue_command(Command::DemolishBuilding { building: site });
    for _ in 0..=RECHECK_TICKS {
        session.tick();
    }
    tools.update(&session, &hover(tile));
    let preview = tools.placement().expect("preview");
    assert!(preview.check.ok, "{:?}", preview.check.errors);
    assert!(preview.access_planned);
    let road = access_road_type(session.state(), &defs).expect("footpath");
    let expected = plan_access_road(session.state(), &defs, kind, origin, Rotation::default(), road).expect("a road in reach");
    assert_eq!(preview.access.as_ref().map(|p| p.positions()), Some(expected.positions()));
}

/// Wall pieces and towers on a wall line are entered from any side: the build tool suggests no
/// access road for them and places them with a plain `PlaceBuilding`.
#[test]
fn wall_pieces_get_no_access_road() {
    let mut session = session();
    let defs = session.shared_defs();
    let stone = defs.resource_id("stone").expect("stone");
    session.queue_command(Command::DebugUnlockAllResearch);
    session.queue_command(Command::DebugGrant { gold: 5_000, goods: vec![kingdoms_sim::defs::ResourceAmount { resource: stone, amount: 200 }] });
    session.tick();
    let wall = defs.building_id("wall").expect("wall");
    assert!(!defs.building(wall).needs_road());
    let home = session.home_tile();
    // A spot whose entrance is free land (a road would be suggested for other buildings).
    let tile = (3..30)
        .flat_map(|r| (-r..=r).flat_map(move |y| (-r..=r).map(move |x| home + IVec2::new(x, y))))
        .find(|p| {
            let check = check_placement(session.state(), &defs, wall, *p, Rotation::default());
            check.ok && check.entrance_status == EntranceStatus::Free
        })
        .expect("a free spot for a wall piece");
    let mut tools = ToolController::new();
    tools.select(Tool::Build { kind: wall });
    tools.update(&session, &hover(tile));
    let preview = tools.placement().expect("preview");
    assert!(preview.check.ok, "{:?}", preview.check.errors);
    assert!(!preview.access_planned && preview.access.is_none(), "no access road for a wall piece");
    assert!(!preview.check.warnings.iter().any(|w| matches!(w, PlacementWarning::NoRoadAccess)));
    let out = click(&mut tools, &session, tile, None);
    assert!(matches!(out.commands.first(), Some(Command::PlaceBuilding { kind, .. }) if *kind == wall), "{:?}", out.commands);
}

#[test]
fn queued_orders_claim_their_tiles_until_the_next_tick() {
    use kingdoms_sim::buildings::{TileBlock, TileStatus};
    // The game is paused: commands wait in the queue and no tick runs.
    let mut session = session();
    let kind = session.defs().building_id("woodcutter").expect("woodcutter");
    let tile = free_spot(&session, "woodcutter");
    let mut tools = ToolController::new();
    tools.select(Tool::Build { kind });
    let base = ToolInput { tile: Some(tile), shift: true, ..Default::default() };
    tools.update(&session, &ToolInput { pressed: true, down: true, ..base });
    let out = tools.update(&session, &ToolInput { released: true, ..base });
    assert_eq!(out.commands.len(), 1);
    let placed = out.commands[0].clone();
    session.queue_command(placed.clone());
    if matches!(placed, Command::PlaceBuildingWithAccess { .. }) {
        assert!(tools.recorded_plan(&placed).is_some(), "the map shows the planned access road");
    }

    // The same spot is now taken by the waiting order: red preview, the click is refused at once.
    tools.update(&session, &base);
    let preview = tools.placement().expect("preview");
    assert!(preview.queued_conflict && !preview.check.ok);
    assert!(preview.check.tile_status.iter().all(|(_, s)| *s == TileStatus::Blocked(TileBlock::Building)));
    tools.update(&session, &ToolInput { pressed: true, down: true, ..base });
    let out = tools.update(&session, &ToolInput { released: true, ..base });
    assert!(out.commands.is_empty());
    assert_eq!(out.message.as_deref(), Some(QUEUED_CONFLICT));
    assert_eq!(out.sounds, vec!["ui_error"]);
    assert_eq!(session.queued_commands().len(), 1);

    // A road on the waiting footprint is refused as well.
    let road = access_road_type(session.state(), session.defs()).expect("footpath");
    tools.select(Tool::Road { road });
    let on_site = ToolInput { tile: Some(tile), ..Default::default() };
    tools.update(&session, &on_site);
    assert!(tools.road_queued_conflict());
    tools.update(&session, &ToolInput { pressed: true, down: true, ..on_site });
    let out = tools.update(&session, &ToolInput { released: true, ..on_site });
    assert!(out.commands.is_empty());
    assert_eq!(out.message.as_deref(), Some(QUEUED_CONFLICT));

    // Once the tick ran, the order is a real site and the recorded plan is dropped.
    session.tick();
    tools.select(Tool::Build { kind });
    tools.update(&session, &base);
    let preview = tools.placement().expect("preview");
    assert!(!preview.queued_conflict && !preview.check.ok, "the site itself blocks the spot now");
    assert!(tools.recorded_plan(&placed).is_none());
}

/// A site no storage reaches is not called suitable: the preview turns amber, not green.
#[test]
fn a_site_out_of_every_storages_reach_is_marked_unsupplied() {
    use crate::view::preview::GhostLook;
    use kingdoms_sim::buildings::PlacementWarning;
    let session = session();
    let defs = session.defs();
    let kind = defs.building_id("woodcutter").expect("woodcutter");
    let tile = free_spot(&session, "woodcutter");
    let size = defs.building(kind).size;
    let mut check = check_placement(session.state(), defs, kind, centred_origin(tile, size), Rotation::default());
    assert!(check.ok && !site_unsupplied(&check), "next to the headquarters: {:?}", check.warnings);
    assert_eq!(GhostLook::of(&check), GhostLook::Ok);
    check.warnings.push(PlacementWarning::NoStorageInReach);
    assert!(site_unsupplied(&check));
    assert_eq!(GhostLook::of(&check), GhostLook::Unsupplied);
    check.ok = false;
    assert_eq!(GhostLook::of(&check), GhostLook::Blocked);
}
