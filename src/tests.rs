use crate::{
    model::{App, Input, InputMode, Preferences, demo_inventory},
    platform::{normalize_apps, normalize_inputs},
    state::{Direction, Effect, Overlay, Page, State, Target},
    storage, view,
};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

fn ready() -> State {
    let mut state = State::new(Preferences::default());
    state.set_inventory(demo_inventory(), None);
    state
}

fn scene(state: &State) -> view::Scene {
    let assets = view::Assets {
        root: PathBuf::new(),
        version: "test".into(),
        metrics: BTreeMap::new(),
        catalog_metrics: BTreeMap::new(),
        active_heading: std::cell::Cell::new(crate::typography::HeadingFont::default()),
        active_text: std::cell::Cell::new(crate::typography::TextFont::default()),
        heading_pct: std::cell::Cell::new(state.preferences.heading_size_pct),
        text_pct: std::cell::Cell::new(state.preferences.text_size_pct),
        artwork: crate::app_artwork::Service::new(false),
    };
    let navigation = view::navigation_for(&assets, state.preferences.language, false);
    view::scene_with_pill(
        state,
        &assets,
        navigation.items[state.page_index()].x,
        navigation.items[state.page_index()].w,
        time::OffsetDateTime::UNIX_EPOCH,
    )
}

fn controls(state: &State) -> Vec<crate::state::Control> {
    scene(state).controls
}

#[test]
fn remote_routes_from_header_and_back_through_settings() {
    let mut state = ready();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::App("netflix".into()));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Nav(0));
    state.open(2);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(0));
    state.activate();
    assert_eq!(state.focus, Target::Setting(0));
    assert!(state.back().is_empty());
    assert_eq!(state.focus, Target::Sidebar(0));
    state.back();
    assert_eq!(state.focus, Target::Nav(2));
    state.back();
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.back(), vec![Effect::Quit]);
}

#[test]
fn hiding_restores_nearest_app_and_hidden_filter_can_restore_it() {
    let mut state = ready();
    state.open(1);
    state.focus = Target::App("netflix".into());
    state.options();
    state.focus = Target::Option(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.hidden.contains(&"netflix".into()));
    assert_eq!(state.focus, Target::App("youtube.leanback.v4".into()));
    state.filter = 5;
    assert_eq!(state.apps().len(), 1);
    state.focus = Target::App("netflix".into());
    state.options();
    state.focus = Target::Option(1);
    state.activate();
    assert!(state.preferences.hidden.is_empty());
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert_eq!(state.filter, 0);
}

#[test]
fn reorder_persists_order_and_keeps_selected_identity() {
    let mut state = ready();
    state.focus = Target::App("netflix".into());
    state.hold();
    assert_eq!(
        state.direction(Direction::Right, &controls(&state)),
        vec![Effect::Save]
    );
    assert_eq!(
        &state.preferences.favorites(&state.inventory.apps)[..2],
        &["youtube.leanback.v4", "netflix"]
    );
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert!(state.back().is_empty());
    assert!(state.moving.is_none());
}

#[test]
fn keyboard_is_modal_and_back_preserves_then_clears_search() {
    let mut state = ready();
    state.open(1);
    state.focus = Target::Search;
    state.activate();
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::Key(_) | Target::CloseDialog))
    );
    state.text("netflix");
    state.delete();
    state.text("x");
    state.back();
    assert_eq!(state.query, "netflix");
    assert_eq!(state.focus, Target::Search);
    assert_eq!(state.apps().len(), 1);
    state.back();
    assert!(state.query.is_empty());
    assert_eq!(state.focus, Target::Nav(1));
    assert_eq!(state.page, Page::Apps);
}

#[test]
fn partial_refresh_preserves_valid_inputs_and_never_invents_tv_apps() {
    let mut state = ready();
    state.set_inventory(
        crate::model::Inventory::default(),
        Some("Inputs unavailable".into()),
    );
    assert!(state.inventory.apps.is_empty());
    assert_eq!(state.inventory.inputs.len(), 3);
    assert_eq!(state.first_content(), Target::Browse);
}

#[test]
fn inventory_filters_system_apps_duplicates_and_unsafe_identifiers() {
    let apps = normalize_apps(&json!([
        {"id":"netflix","title":" Netflix\n"}, {"id":"netflix","title":"Duplicate"},
        {"id":"bad;command","title":"Bad"}, {"id":"com.webos.app.hidden","title":"Internal"},
        {"id":"com.webos.app.livetv","visible":false,"title":"Live TV"},
        {"id":"com.luma.launcher.native","visible":true},
        {"id":"com.webos.app.hdmi1","visible":true},
        {"id":"hidden","visible":"false"}
    ]))
    .unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(
        apps.iter().find(|a| a.id == "netflix").unwrap().title,
        "Netflix"
    );
    assert!(normalize_apps(&json!({"apps":[]})).is_err());
    let inputs = normalize_inputs(&json!([
        {"appId":"com.webos.app.hdmi1","id":"HDMI_1","port":1,"connected":true},
        {"appId":"com.webos.app.hdmi2","id":"HDMI_4"},
        {"appId":"com.webos.app.hdmi3"}, {"appId":"com.webos.app.hdmi3"},
        {"appId":"com.webos.app.hdmi4","label":"Console"}
    ]))
    .unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[1].connected, None);
}

#[test]
fn inventory_visibility_cases_match_the_launcher_filters() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/fixtures/app-visibility.json")).unwrap();
    let inventory: Vec<_> = cases.iter().map(|case| case["app"].clone()).collect();
    let actual: std::collections::BTreeSet<_> = normalize_apps(&json!(inventory))
        .unwrap()
        .into_iter()
        .map(|app| app.id)
        .collect();
    let expected: std::collections::BTreeSet<_> = cases
        .iter()
        .filter(|case| case["include"] == true)
        .map(|case| case["app"]["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn asset_loading_and_unknown_app_cards_do_not_require_collected_artwork() {
    let root = std::env::temp_dir().join(format!("luma-assets-no-dumps-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("version.txt"), "test").unwrap();
    std::fs::write(root.join("font-metrics.json"), "{}").unwrap();
    for webos in [false, true] {
        let assets = view::Assets::load(root.clone(), webos).unwrap();
        let mut state = State::new(Preferences::default());
        state.set_inventory(
            crate::model::Inventory {
                apps: vec![App::new("com.example.regional", "Regional App")],
                inputs: vec![],
            },
            None,
        );
        state.open(1);
        let scene = view::scene_with_pill(
            &state,
            &assets,
            800.,
            100.,
            time::OffsetDateTime::UNIX_EPOCH,
        );
        assert!(scene.draw.iter().any(|draw| matches!(draw,
            crate::renderer::Draw::TintedImage(path, ..) if path.ends_with("ui-icons/generic.lvec"))));
        assert!(scene.draw.iter().any(|draw| matches!(draw,
            crate::renderer::Draw::Text(value, ..) if value == "Regional App")));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn auto_input_visibility_requires_a_positive_connection_report() {
    let mut preferences = Preferences::default();
    let mut input = Input {
        id: "com.webos.app.hdmi1".into(),
        title: "Console".into(),
        port: "HDMI 1".into(),
        connected: None,
        signal: None,
    };
    assert!(!preferences.input_visible(&input));
    input.connected = Some(true);
    assert!(preferences.input_visible(&input));
    preferences
        .input_modes
        .insert(input.id.clone(), InputMode::Off);
    assert!(!preferences.input_visible(&input));
    preferences
        .input_modes
        .insert(input.id.clone(), InputMode::On);
    input.connected = Some(false);
    assert!(preferences.input_visible(&input));
}

#[test]
fn preferences_round_trip_and_corrupt_data_is_not_silently_overwritten() {
    let directory = std::env::temp_dir().join(format!("luma-native-test-{}", std::process::id()));
    let path = directory.join("preferences.json");
    let mut preferences = Preferences {
        favorites: Some(vec![]),
        background: "forest".into(),
        ..Preferences::default()
    };
    preferences
        .input_modes
        .insert("com.webos.app.hdmi1".into(), InputMode::Off);
    storage::save(&path, &preferences).unwrap();
    let loaded = storage::load(&path).unwrap();
    assert_eq!(loaded.favorites, Some(vec![]));
    assert_eq!(loaded.background, "forest");
    assert_eq!(loaded.input_modes["com.webos.app.hdmi1"], InputMode::Off);
    std::fs::write(&path, "broken").unwrap();
    assert!(storage::load(&path).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn home_clip_extends_to_screen_bottom_without_changing_other_pages() {
    let mut state = ready();
    for (page, expected_bottom, expected_left, expected_width) in [
        (Page::Home, 1080., 0., 1920.),
        (Page::Apps, 1080., 64., 1792.),
        (Page::Settings, 994.4, 0., 1920.),
    ] {
        state.page = page;
        assert_eq!(state.main_bottom(), expected_bottom);
        let clip = scene(&state)
            .draw
            .into_iter()
            .find_map(|draw| match draw {
                crate::renderer::Draw::Clip(Some(rect)) => Some(rect),
                _ => None,
            })
            .expect("page content clip");
        assert_eq!(clip.x, expected_left);
        assert_eq!(clip.y, if page == Page::Settings { 0. } else { 108.8 });
        assert_eq!(clip.w, expected_width);
        assert!((clip.y + clip.h - expected_bottom).abs() < 0.01);
        let inset = if page == Page::Settings { 12. } else { 64. };
        assert_eq!(state.focus_bottom(), expected_bottom - inset);
    }
    state.page = Page::Home;
    state.moving = Some("test".to_string());
    assert_eq!(state.main_bottom(), 994.4);
}

#[test]
fn favorites_scroll_uses_physical_edges_and_card_aligned_focus() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.page = Page::Home;
    state.preferences.favorites = Some(
        state
            .inventory
            .apps
            .iter()
            .take(8)
            .map(|app| app.id.clone())
            .collect(),
    );
    let target_id = state.apps()[1].id.clone();
    state.focus(Target::App(target_id.clone()), &controls(&state));
    let card = controls(&state)
        .into_iter()
        .find(|control| control.target == Target::App(target_id.clone()))
        .expect("focused favorite card");
    assert!(
        (card.rect.x - (crate::state::HOME_STRIP_LEFT + crate::state::HOME_TILE_PITCH)).abs()
            < 0.01
    );
    assert_eq!(state.home_scroll_goal, 0.);
    assert!(
        (state.home_scroll / crate::state::HOME_TILE_PITCH)
            .fract()
            .abs()
            < 0.01
    );

    state.home_scroll = 0.;
    state.home_scroll_goal = 0.;
    state.scroll_favorites(crate::state::HOME_TILE_PITCH * 0.6);
    state.tick();
    let expected_scroll = state.home_scroll_max().min(crate::state::HOME_TILE_PITCH);
    assert!((state.home_scroll - expected_scroll).abs() < 0.01);

    let scene = scene(&state);
    let clip = scene
        .draw
        .iter()
        .find_map(|draw| match draw {
            crate::renderer::Draw::Clip(Some(rect)) => Some(*rect),
            _ => None,
        })
        .expect("vertical body clip");
    assert_eq!(clip.x, crate::state::HOME_VIEWPORT_LEFT);
    assert_eq!(
        clip.w,
        crate::state::HOME_VIEWPORT_RIGHT - crate::state::HOME_VIEWPORT_LEFT
    );
    let first = scene
        .controls
        .iter()
        .find(|control| control.target == Target::App(state.apps()[0].id.clone()))
        .expect("first favorite control");
    if state.home_scroll > 0. {
        assert!(first.rect.x < crate::state::HOME_VIEWPORT_LEFT);
        assert!(first.rect.x + first.rect.w > crate::state::HOME_VIEWPORT_LEFT);
    } else {
        assert_eq!(first.rect.x, crate::state::HOME_STRIP_LEFT);
    }
}

#[test]
fn favorites_remote_focus_scrolls_only_at_physical_edges() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.page = Page::Home;
    state.preferences.favorites = Some(
        state
            .inventory
            .apps
            .iter()
            .take(8)
            .map(|app| app.id.clone())
            .collect(),
    );
    let ids: Vec<_> = state.apps().iter().map(|app| app.id.clone()).collect();
    state.focus(Target::App(ids[0].clone()), &controls(&state));

    for id in &ids[1..=5] {
        state.direction(Direction::Right, &controls(&state));
        assert_eq!(state.focus, Target::App(id.clone()));
        assert_eq!(state.home_scroll_goal, 0.);
    }

    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::App(ids[6].clone()));
    assert!(
        (state.home_scroll_goal - crate::state::HOME_TILE_PITCH).abs() < 0.01,
        "edge focus should advance by one card pitch"
    );
    assert!((state.home_scroll - state.home_scroll_goal).abs() < 0.01);

    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::App(ids[5].clone()));
    assert!(
        (state.home_scroll_goal - crate::state::HOME_TILE_PITCH).abs() < 0.01,
        "moving back inside the visible row must not undo the scroll"
    );
}

#[test]
fn home_inputs_are_header_circles_left_of_settings() {
    let mut state = ready();
    for input in &state.inventory.inputs {
        state
            .preferences
            .input_modes
            .insert(input.id.clone(), InputMode::On);
    }
    let home = controls(&state);
    let settings = home
        .iter()
        .find(|c| c.target == Target::TvSettings)
        .expect("TV settings circle");
    let circles: Vec<_> = home
        .iter()
        .filter(|c| matches!(c.target, Target::Input(_)))
        .collect();
    assert!(!circles.is_empty());
    for circle in circles {
        assert_eq!(circle.rect.h, 68.);
        assert_eq!(circle.rect.y, settings.rect.y);
        assert!(circle.rect.x < settings.rect.x);
    }
}
#[test]
fn gallery_and_grid_navigation_scroll_to_keep_focus_visible() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(1);
    state.activate();
    assert_eq!(
        state.settings_level,
        crate::state::SettingsLevel::WallpaperPhotos
    );
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(4));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(8));
    state.back();
    assert_eq!(state.focus, Target::WallpaperMenu(1));
    state.back();
    assert_eq!(state.focus, Target::Sidebar(1));
    state.open(1);
    let last = state.apps().last().unwrap().id.clone();
    state.focus(Target::App(last), &controls(&state));
    assert!(state.scroll_goal > 0.);
}

#[test]
fn cancelling_options_preserves_identity_and_does_not_save() {
    let mut state = ready();
    state.focus = Target::App("netflix".into());
    state.options();
    assert!(matches!(state.overlay, Some(Overlay::Options { .. })));
    state.focus = Target::CloseDialog;
    assert!(state.activate().is_empty());
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert!(state.overlay.is_none());
}

#[test]
fn pointer_focus_does_not_scroll_the_clicked_control() {
    let mut state = ready();
    state.open(1);
    state.scroll = 150.;
    state.pointer_focus(Target::App("netflix".into()));
    assert_eq!(state.scroll, 150.);
    state.focus(Target::Nav(1), &controls(&state));
    assert_eq!(state.scroll_goal, 0.);
    assert_eq!(state.scroll, 150.);
    state.tick();
    assert!(state.scroll < 150.);
}

#[test]
fn gallery_right_crosses_rows_in_gallery_order() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::Background(2);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(3));
}

#[test]
fn reordering_skips_hidden_and_unavailable_favorite_slots() {
    let mut state = ready();
    state.preferences.favorites = Some(vec![
        "netflix".into(),
        "missing".into(),
        "youtube.leanback.v4".into(),
        "amazon".into(),
    ]);
    state.preferences.hidden.push("youtube.leanback.v4".into());
    state.focus = Target::App("netflix".into());
    state.hold();
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(
        state.preferences.favorites.unwrap(),
        vec!["amazon", "missing", "youtube.leanback.v4", "netflix"]
    );
    assert_eq!(state.focus, Target::App("netflix".into()));
}

#[test]
fn focused_navigation_tabs_switch_pages_immediately() {
    let mut state = ready();
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    assert_eq!(state.focus, Target::Nav(1));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.page, Page::Settings);
    assert_eq!(state.focus, Target::Nav(2));
}

#[test]
fn greeting_subpage_opens_the_keyboard_and_back_restores_its_parent() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(6);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Greeting));
    assert_eq!(state.focus, Target::GreetingStyle);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::GreetingName);
    assert!(!state.text_input_active());
    assert!(state.activate().is_empty());
    assert!(state.text_input_active());
    state.text("Ana");
    assert_eq!(state.editing_text(), "Ana");
    assert!(state.preferences.greeting_name.is_empty());
    assert_eq!(state.finish_text_input(), vec![Effect::Save]);
    assert_eq!(state.focus, Target::GreetingName);
    assert!(state.finish_text_input().is_empty());
    assert_eq!(
        crate::i18n::Language::En.greeting(9, &state.preferences.greeting_name),
        "Good morning, Ana."
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Greeting));
    assert!(state.back().is_empty());
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(6));
}

#[test]
fn greeting_name_replacement_commits_on_every_keyboard_completion_route() {
    for route in 0..3 {
        let mut state = ready();
        state.preferences.greeting_name = "Ana".into();
        state.open(2);
        state.focus = Target::Setting(6);
        state.activate();
        state.direction(Direction::Down, &controls(&state));
        state.activate();
        assert!(state.editing_text().is_empty());
        state.text("  BobX ");
        state.delete_surrounding_text(-3, 3);
        assert_eq!(state.editing_text(), "  Bo");
        state.text("b  ");
        assert_eq!(state.preferences.greeting_name, "Ana");
        let effects = match route {
            0 => {
                state.overlay = Some(Overlay::SystemKeyboard);
                state.finish_text_input()
            }
            1 => state.back(),
            _ => {
                state.focus = Target::Key(crate::state::KEYS.len() - 1);
                state.activate()
            }
        };
        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(state.preferences.greeting_name, "Bob");
        assert_eq!(state.focus, Target::GreetingName);
        assert!(state.finish_text_input().is_empty());
        state.activate();
        state.text("   ");
        assert!(state.back().is_empty());
        assert_eq!(state.preferences.greeting_name, "Bob");
    }
}

#[test]
fn remote_can_clear_a_saved_greeting_name_without_moving_the_controls() {
    let mut state = ready();
    state.preferences.greeting_name = "Ana".into();
    state.open(2);
    state.focus = Target::Setting(6);
    state.activate();
    state.direction(Direction::Down, &controls(&state));
    let before: Vec<_> = controls(&state)
        .iter()
        .map(|c| (c.target.clone(), c.rect.y))
        .collect();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::GreetingNameClear);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.greeting_name.is_empty());
    assert_eq!(state.focus, Target::GreetingName);
    let after: Vec<_> = controls(&state)
        .iter()
        .map(|c| (c.target.clone(), c.rect.y))
        .collect();
    assert_eq!(
        before.iter().map(|(target, _)| target).collect::<Vec<_>>(),
        after.iter().map(|(target, _)| target).collect::<Vec<_>>()
    );
    assert!(
        before
            .iter()
            .zip(after.iter())
            .all(|(a, b)| (a.1 - b.1).abs() < 1.)
    );
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(6));
    assert!(state.subpage.is_none());
}

#[test]
fn app_name_settings_use_independent_defaults_including_with_legacy_preferences() {
    let old: Preferences = serde_json::from_value(json!({"show_names": true})).unwrap();
    assert!(!old.show_home_names);
    assert!(old.show_app_names);
    let mut state = ready();
    assert!(!state.preferences.show_home_names);
    assert!(state.preferences.show_app_names);
    state.open(2);
    state.focus = Target::Setting(0);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::AppNames));
    assert_eq!(state.focus, Target::AppNamesHome);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_home_names);
    assert!(state.preferences.show_app_names);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::AppNamesApps);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_home_names);
    assert!(!state.preferences.show_app_names);
    assert_eq!(state.subpage, Some(crate::state::Subpage::AppNames));
    let restored: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    assert!(restored.show_home_names);
    assert!(!restored.show_app_names);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(0));
    assert!(state.subpage.is_none());
}

#[test]
fn home_and_apps_render_only_their_own_caption_preference() {
    let mut state = ready();
    for (home, apps) in [(false, true), (true, false), (true, true), (false, false)] {
        state.preferences.show_home_names = home;
        state.preferences.show_app_names = apps;
        for (page, expected) in [(0, home), (1, apps)] {
            state.open(page);
            let scene = scene(&state);
            let caption = scene.draw.iter().any(
                |draw| matches!(draw, crate::renderer::Draw::Text(value, ..) if value == "Netflix"),
            );
            assert_eq!(caption, expected, "Home={home}, Apps={apps}, page={page}");
            let card = scene
                .controls
                .iter()
                .find(|c| c.target == Target::App("netflix".into()))
                .unwrap();
            assert_eq!(
                card.rect.h > 154.4,
                expected,
                "Hidden captions must not reserve space"
            );
        }
    }
}

#[test]
fn input_name_actions_only_highlight_the_focused_row_and_keep_the_saved_mode() {
    let mut state = ready();
    legacy_look(&mut state.preferences);
    state.preferences.reduced_motion = true;
    state
        .preferences
        .set_input_name("com.webos.app.hdmi1", "Console");
    state.open(2);
    state.focus = Target::Sidebar(crate::state::INPUTS_SECTION);
    state.activate();
    state.focus = Target::InputMode("com.webos.app.hdmi1".into());
    state.activate();
    for target in [Target::InputRename, Target::InputNameReset] {
        state.pointer_focus(target.clone());
        let screen = scene(&state);
        assert!(screen.controls.iter().any(|c| c.target == target));
        assert!(screen.draw.iter().any(|d| matches!(d,
            crate::renderer::Draw::Shape(_, 16., 0., rgba) if rgba[3] > 0.99)));
        assert_eq!(
            state
                .preferences
                .input_modes
                .get("com.webos.app.hdmi1")
                .copied()
                .unwrap_or_default(),
            InputMode::Auto
        );
        assert!(screen.draw.iter().any(|d| matches!(d,
            crate::renderer::Draw::Text(value, ..) if value == "Clear name")));
    }
}

#[test]
fn clock_choices_only_highlight_focus_while_preserving_saved_checkmarks() {
    let mut state = ready();
    legacy_look(&mut state.preferences);
    state.preferences.reduced_motion = true;
    state.preferences.show_seconds = true;
    state.open(2);
    state.focus = Target::Sidebar(0);
    state.activate();
    state.focus = Target::Setting(1);
    state.activate();
    state.focus = Target::ClockTimeFormat;
    state.activate();
    for index in 0..2 {
        state.pointer_focus(Target::Choice(index));
        let screen = scene(&state);
        assert!(
            screen
                .controls
                .iter()
                .any(|c| c.target == Target::Choice(index))
        );
        assert!(screen.draw.iter().any(|d| matches!(d,
            crate::renderer::Draw::Shape(_, 16., 0., rgba) if rgba[3] > 0.99)));
        assert!(state.preferences.show_seconds);
    }
}

#[test]
fn clock_format_subpage_saves_and_restores_its_parent_row() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(1);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::ClockFormat));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.hour_24);
    assert!(!state.preferences.show_seconds);
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    state.direction(Direction::Left, &controls(&state));
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(1));
    state.activate();
    state.activate();
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    state.back();
    assert_eq!(state.focus, Target::Setting(1));
    assert!(state.subpage.is_none());
}

#[test]
fn time_format_selects_seconds_without_changing_the_hour_format() {
    let mut state = ready();
    state.preferences.hour_24 = true;
    state.open(2);
    state.focus = Target::Setting(1);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert!(!state.preferences.show_seconds);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::TimeFormat));
    assert_eq!(state.focus, Target::Choice(0));
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Choice(2))
    );
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(1));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_seconds);
    assert!(state.preferences.hour_24);
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.activate();
    assert_eq!(state.focus, Target::Choice(1));
    state.back();
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.activate();
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.show_seconds);
    assert!(state.preferences.hour_24);
    state.back();
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(1));
}

#[test]
fn system_category_owns_refresh_and_quit_and_restores_sidebar_focus() {
    let mut state = ready();
    state.open(2);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| matches!(c.target, Target::Setting(3 | 5)))
    );
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Sidebar(crate::state::SYSTEM_SECTION))
    );
    state.focus = Target::Sidebar(crate::state::SYSTEM_SECTION - 1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(crate::state::SYSTEM_SECTION));
    assert_eq!(state.section, crate::state::SYSTEM_SECTION);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Setting(3));
    assert_eq!(state.activate(), vec![Effect::Reload]);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Setting(5));
    assert_eq!(state.activate(), vec![Effect::Quit]);
    state.back();
    assert_eq!(state.focus, Target::Sidebar(crate::state::SYSTEM_SECTION));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(0));
    assert_eq!(state.section, 0);
}

#[test]
fn input_visibility_has_explicit_choices_and_returns_to_the_same_input() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    let id = "com.webos.app.hdmi1".to_string();
    state.focus = Target::InputMode(id.clone());
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.input_modes[&id], InputMode::Off);
    assert_eq!(state.focus, Target::InputMode(id.clone()));
    assert!(!state.preferences.input_visible(&state.inventory.inputs[0]));
    state.activate();
    state.back();
    assert_eq!(state.focus, Target::InputMode(id));
    assert!(state.subpage.is_none());
}

#[test]
fn background_entry_restores_the_applied_background() {
    let mut state = ready();
    state.open(2);
    state.preferences.background = "forest".into();
    state.focus = Target::Sidebar(1);
    state.activate();
    assert_eq!(state.focus, Target::WallpaperMenu(0));
    assert_eq!(state.settings_level, crate::state::SettingsLevel::Section);
    state.activate();
    assert_eq!(
        state.settings_level,
        crate::state::SettingsLevel::WallpaperType
    );
    state.back();
    assert_eq!(state.focus, Target::WallpaperMenu(0));
    state.back();
    assert_eq!(state.focus, Target::Sidebar(1));
    assert_eq!(state.preferences.background, "forest");
}

#[test]
fn categories_and_search_have_clear_actions_and_conditional_hidden_control() {
    let mut state = ready();
    state.open(1);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Filter(5))
    );
    state.focus = Target::Search;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Filter(4));
    state.preferences.hidden.push("netflix".into());
    state.focus = Target::Search;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Filter(5));
    state.activate();
    assert_eq!(state.apps().len(), 1);
    state.query = "YouTube".into();
    state.focus = Target::ClearSearch;
    state.activate();
    assert!(state.query.is_empty());
    assert_eq!(state.focus, Target::Search);
}

#[test]
fn a_notice_does_not_escape_modal_focus_and_closing_restores_the_app() {
    let mut state = ready();
    state.open(1);
    state.notify("Saved");
    state.focus = Target::App("netflix".into());
    state.options();
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::Option(_) | Target::CloseDialog))
    );
    state.focus = Target::Option(1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Option(1));
    state.back();
    assert_eq!(state.focus, Target::App("netflix".into()));
}

#[test]
fn remote_toolbar_focus_normalizes_scroll_but_hover_does_not() {
    let mut state = ready();
    state.open(1);
    state.scroll = 100.;
    state.pointer_focus(Target::Filter(1));
    assert_eq!(state.scroll, 100.);
    state.focus(Target::Filter(2), &controls(&state));
    assert_eq!(state.scroll_goal, 0.);
    state.tick();
    assert!(state.scroll <= 100.);
}

#[test]
fn long_captions_expand_rows_without_overlapping_the_next_row() {
    let mut state = ready();
    state.open(1);
    state.inventory.apps.iter_mut().find(|app| app.id == "netflix").unwrap().title = "A very long installed application title that needs more than two lines to remain fully readable".into();
    let targets = controls(&state);
    let title = targets
        .iter()
        .find(|c| c.target == Target::App("netflix".into()))
        .unwrap();
    let next_id = state.apps()[6].id.clone();
    let next = targets
        .iter()
        .find(|c| c.target == Target::App(next_id.clone()))
        .unwrap();
    assert!(title.rect.h > 240.);
    assert!(next.rect.y > title.rect.y + title.rect.h);
}

#[test]
fn reduced_motion_disables_panel_wallpaper_and_notification_animation() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open(2);
    state.focus = Target::Setting(1);
    state.activate();
    assert!(!state.animating());
    state.back();
    state.focus = Target::Background(6);
    state.activate();
    state.notify("Saved");
    assert!(!state.animating());
}

#[test]
fn pinning_a_hidden_app_restores_it_and_leaves_hidden_when_empty() {
    let mut state = ready();
    state.open(1);
    state.filter = 5;
    state.preferences.hidden.push("org.jellyfin.webos".into());
    state.focus = Target::App("org.jellyfin.webos".into());
    state.options();
    state.activate();
    assert!(state.preferences.hidden.is_empty());
    assert_eq!(state.filter, 0);
    assert!(
        state
            .preferences
            .favorites(&state.inventory.apps)
            .contains(&"org.jellyfin.webos".into())
    );
    assert_eq!(state.focus, Target::App("org.jellyfin.webos".into()));
}

#[test]
fn weather_location_keyboard_back_and_sidebar_routes_restore_focus() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(crate::state::WEATHER_SECTION);
    state.section = crate::state::WEATHER_SECTION;
    // The sidebar row is Widgets now; Weather is its fourth row.
    state.activate();
    assert_eq!(state.focus, Target::WidgetRow(0));
    for _ in 0..3 {
        state.direction(Direction::Down, &controls(&state));
    }
    assert_eq!(state.focus, Target::WidgetRow(3));
    state.activate();
    assert_eq!(state.focus, Target::WeatherToggle);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherLocation);
    state.activate();
    assert_eq!(state.focus, Target::WeatherQuery);
    state.activate();
    state.text("Vienna");
    state.back();
    assert_eq!(state.focus, Target::WeatherSearch);
    assert_eq!(state.weather.query, "Vienna");
    assert_eq!(state.subpage, Some(crate::state::Subpage::WeatherLocation));
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::WeatherLocation);
    assert!(state.subpage.is_none());
}

#[test]
fn weather_units_save_without_replacing_celsius_cache_and_hide_keeps_city() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WEATHER_SECTION;
    let place = crate::weather::Location {
        name: "Vienna".into(),
        region: "Vienna".into(),
        country: "Austria".into(),
        latitude: 48.2085,
        longitude: 16.3721,
    };
    state.preferences.weather_location = Some(place.clone());
    state.weather.cache = Some(crate::weather::Cache {
        key: place.key(),
        updated_at: 1000,
        reading: crate::weather::Reading {
            temperature: 14.2,
            code: 2,
            is_day: true,
            wind_speed: 0.,
            temperature_3h_ago: None,
            hours: [None; 5],
        },
    });
    state.focus = Target::WeatherTemperature;
    state.activate();
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.temperature_unit,
        crate::weather::Unit::Fahrenheit
    );
    assert_eq!(
        state.weather.cache.as_ref().unwrap().reading.temperature,
        14.2
    );
    assert_eq!(state.focus, Target::WeatherTemperature);
    state.focus = Target::WeatherToggle;
    state.activate();
    assert!(!state.preferences.show_weather);
    assert_eq!(state.preferences.weather_location, Some(place));
}

#[test]
fn removing_weather_location_clears_cache_and_cancels_pending_results() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WEATHER_SECTION;
    state.focus = Target::WeatherLocation;
    state.activate();
    state.focus = Target::WeatherClear;
    let effects = state.activate();
    assert_eq!(
        effects,
        vec![
            Effect::Save,
            Effect::Weather(crate::weather::Command::ClearCache)
        ]
    );
    assert!(state.weather.cache.is_none());
    assert!(state.preferences.weather_location.is_none());
    assert_eq!(state.focus, Target::WeatherSearch);
}

#[test]
fn language_selection_restores_its_parent_and_is_saved_with_preferences() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(4);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Language));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.language, crate::i18n::Language::De);
    assert_eq!(state.focus, Target::Setting(4));
    state.activate();
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(4));
}

#[test]
fn remote_can_select_and_restore_every_language() {
    for (index, language) in crate::i18n::Language::ALL.into_iter().enumerate() {
        let mut state = ready();
        state.open(2);
        state.focus = Target::Setting(4);
        state.activate();
        for _ in 0..index {
            state.direction(Direction::Down, &controls(&state));
        }
        assert_eq!(state.focus, Target::Choice(index));
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert_eq!(state.preferences.language, language);
        assert_eq!(state.focus, Target::Setting(4));

        let saved = serde_json::to_value(&state.preferences).unwrap();
        let mut restored = State::new(serde_json::from_value(saved).unwrap());
        restored.open(2);
        restored.focus = Target::Setting(4);
        restored.activate();
        assert_eq!(restored.preferences.language, language);
        assert_eq!(restored.focus, Target::Choice(index));
        restored.direction(Direction::Down, &controls(&restored));
        assert_eq!(
            restored.focus,
            Target::Choice((index + 1) % crate::i18n::Language::ALL.len())
        );
    }
}

#[test]
fn unsupported_language_preferences_recover_without_losing_other_choices() {
    let preferences: Preferences = serde_json::from_value(
        json!({"language":"unknown","favorites":["netflix"],"show_home_names":true}),
    )
    .unwrap();
    assert_eq!(preferences.language, crate::i18n::Language::En);
    assert!(preferences.show_home_names);
    assert_eq!(preferences.favorites, Some(vec!["netflix".into()]));
}

#[test]
fn unrelated_app_ids_and_titles_keep_their_installed_artwork() {
    for (id, title) in [
        ("com.tvweb.dashboard", "Glasshouse"),
        ("com.example.ardent", "Notes"),
        ("com.example.3ard", "Notes"),
        ("com.example.ard3", "Notes"),
        ("com.example.plexus", "Notes"),
        ("com.example.media", "Standard dashboard"),
        ("com.example.media", "netflixer"),
        ("com.example.media", "preNetflix"),
        ("amazon.alexa.view", "Amazon Alexa"),
        ("com.example.spotify", "Spotify tools"),
        ("com.example.media", "Plex"),
        ("com.example.browser", "Web Browser"),
        ("com.example.livetv", "Live TV"),
    ] {
        let app = App::new(id, title);
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            ("generic", "Utilities"),
            "An unrelated application must not be rebranded: {id} / {title}"
        );
    }
}

#[test]
fn curated_branding_does_not_use_title_words_or_partial_id_matches() {
    for (id, title) in [
        ("com.example.ard_more", "Notes"),
        ("com.example.more-ard", "Notes"),
        ("com.example.ARD", "Notes"),
        ("com.example.media", "ARD"),
        ("com.example.media", "(ARD)"),
        ("com.example.media", "ARD미디어테크"),
        ("com.example.media", "아에르데ARD"),
        ("com.example.media", "아에르데ARD미디어테크"),
        ("com.example.dashboard", "ARD"),
        ("com.example.media", "standard ardent ARD"),
    ] {
        let app = App::new(id, title);
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            ("generic", "Utilities"),
            "Only an exact app ID may select curated artwork: {id} / {title}"
        );
    }
}

#[test]
fn known_app_ids_preserve_the_curated_brands_and_categories() {
    let apps = demo_inventory().apps;
    let expected = [
        ("com.webos.app.livetv", "live-tv", "Streaming"),
        ("netflix", "netflix", "Streaming"),
        ("youtube.leanback.v4", "youtube", "Streaming"),
        ("com.apple.appletv", "apple-tv", "Streaming"),
        ("com.disney.disneyplus-prod", "disney-plus", "Streaming"),
        ("amazon", "prime-video", "Streaming"),
        ("plex", "plex", "Streaming"),
        ("spotify-beehive", "spotify", "Music"),
        ("org.jellyfin.webos", "jellyfin", "Streaming"),
        ("org.xbmc.kodi", "kodi", "Streaming"),
        ("com.limelight.webos", "moonlight", "Gaming"),
        ("org.webosbrew.hbchannel", "homebrew", "Utilities"),
        ("com.webos.app.browser", "browser", "Utilities"),
        ("com.arte.tv", "arte", "Streaming"),
        ("de.zdf.app.zdfm3", "zdf", "Streaming"),
        ("ard.mediathek", "das-erste", "Streaming"),
    ];
    for (id, brand, category) in expected {
        let app = apps.iter().find(|app| app.id == id).unwrap();
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            (brand, category),
            "Known app {id} must retain its curated brand and category"
        );
    }
}

#[test]
fn hdmi_plug_in_reports_the_cable_state_when_connected_only_means_present() {
    let inputs = normalize_inputs(&json!([
        {"id":"HDMI_1","label":"HDMI 1","port":1,"connected":true,"appId":"com.webos.app.hdmi1","hdmiPlugIn":false},
        {"id":"HDMI_2","label":"AVR-X1800H","port":2,"connected":true,"appId":"com.webos.app.hdmi2","hdmiPlugIn":true},
        {"id":"HDMI_3","label":"HDMI 3","port":3,"connected":true,"appId":"com.webos.app.hdmi3","hdmiPlugIn":false},
        {"id":"HDMI_4","label":"HDMI 4","port":4,"connected":true,"appId":"com.webos.app.hdmi4","hdmiPlugIn":false}
    ]))
    .unwrap();
    let connected: Vec<_> = inputs
        .iter()
        .map(|input| (input.id.as_str(), input.connected))
        .collect();
    assert_eq!(
        connected,
        [
            ("com.webos.app.hdmi1", Some(false)),
            ("com.webos.app.hdmi2", Some(true)),
            ("com.webos.app.hdmi3", Some(false)),
            ("com.webos.app.hdmi4", Some(false)),
        ]
    );
    assert_eq!(inputs[1].title, "AVR-X1800H");
    let preferences = Preferences::default();
    let visible: Vec<_> = inputs
        .iter()
        .filter(|input| preferences.input_visible(input))
        .map(|input| input.id.as_str())
        .collect();
    assert_eq!(visible, ["com.webos.app.hdmi2"]);
}

#[test]
fn inputs_without_hdmi_plug_in_keep_the_connected_report() {
    let inputs = normalize_inputs(&json!([
        {"appId":"com.webos.app.hdmi1","id":"HDMI_1","port":1,"connected":true},
        {"appId":"com.webos.app.hdmi2","id":"HDMI_2","port":2,"connected":false},
        {"appId":"com.webos.app.hdmi3","connected":true,"hdmiPlugIn":"yes"},
        {"appId":"com.webos.app.hdmi4"}
    ]))
    .unwrap();
    let connected: Vec<_> = inputs.iter().map(|input| input.connected).collect();
    assert_eq!(connected, [Some(true), Some(false), Some(true), None]);
}

#[test]
fn input_names_are_sanitized_and_old_preferences_load_without_them() {
    let old: Preferences = serde_json::from_value(json!({"background": "forest"})).unwrap();
    assert!(old.input_names.is_empty());
    let mut preferences: Preferences = serde_json::from_value(json!({
        "input_names": {
            "com.webos.app.hdmi1": "  Play\nStation\t5  ",
            "com.webos.app.hdmi2": "   ",
            "bad;id": "Ignored",
            "com.webos.app.hdmi3": "A".repeat(60),
        }
    }))
    .unwrap();
    preferences.sanitize();
    assert_eq!(
        preferences.input_names,
        BTreeMap::from([
            (
                "com.webos.app.hdmi1".to_string(),
                "PlayStation5".to_string()
            ),
            (
                "com.webos.app.hdmi3".to_string(),
                "A".repeat(crate::model::INPUT_NAME_LIMIT)
            ),
        ])
    );
    assert!(preferences.set_input_name("com.webos.app.hdmi2", " DENON "));
    assert!(!preferences.set_input_name("com.webos.app.hdmi2", "DENON"));
    assert_eq!(preferences.input_names["com.webos.app.hdmi2"], "DENON");
    assert!(preferences.set_input_name("com.webos.app.hdmi2", ""));
    assert!(!preferences.input_names.contains_key("com.webos.app.hdmi2"));
    assert!(!preferences.set_input_name("bad;id", "Name"));
}

#[test]
fn custom_input_names_keep_the_label_port_and_status_as_detail() {
    let mut preferences = Preferences::default();
    let input = Input {
        id: "com.webos.app.hdmi2".into(),
        title: "AVR-X1800H".into(),
        port: "HDMI 2".into(),
        connected: Some(true),
        signal: None,
    };
    let language = preferences.language;
    let status = |preferences: &Preferences| {
        view::input_status(&input, preferences.input_name(&input).is_some(), language)
    };
    assert_eq!(preferences.input_title(&input), "HDMI 2");
    assert_eq!(status(&preferences), "AVR-X1800H · Connected");
    preferences.set_input_name(&input.id, "DENON");
    assert_eq!(preferences.input_title(&input), "DENON");
    assert_eq!(status(&preferences), "AVR-X1800H · HDMI 2 · Connected");
    let plain = Input {
        title: "HDMI 2".into(),
        ..input.clone()
    };
    assert_eq!(
        view::input_status(&plain, true, language),
        "HDMI 2 · Connected"
    );
    assert_eq!(view::input_status(&plain, false, language), "Connected");
}

#[test]
fn remote_renames_and_resets_an_input_from_its_settings_page() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(crate::state::INPUTS_SECTION);
    state.activate();
    let id = "com.webos.app.hdmi2".to_string();
    state.focus = Target::InputMode(id.clone());
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::InputRename);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::InputNameReset)
    );
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::Keyboard)));
    state.text("DENONX");
    state.delete();
    assert_eq!(state.back(), vec![Effect::Save]);
    assert_eq!(state.focus, Target::InputRename);
    assert_eq!(state.preferences.input_names[&id], "DENON");
    assert_eq!(
        state.preferences.input_title(&state.inventory.inputs[1]),
        "DENON"
    );
    // Dismissing an empty keyboard keeps the saved name.
    state.activate();
    assert!(state.finish_text_input().is_empty());
    assert_eq!(state.preferences.input_names[&id], "DENON");
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::InputNameReset);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.input_names.is_empty());
    assert_eq!(state.focus, Target::InputRename);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Choice(2));
    state.focus = Target::InputRename;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::InputMode(id));
    assert!(state.subpage.is_none());
}

#[test]
fn keyboard_done_saves_a_typed_input_name() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    state.focus = Target::InputMode("com.webos.app.hdmi1".into());
    state.activate();
    state.focus = Target::InputRename;
    state.activate();
    state.text("PS5");
    state.focus = Target::Key(
        crate::state::KEYS
            .iter()
            .position(|k| *k == "Done")
            .unwrap(),
    );
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.input_names["com.webos.app.hdmi1"], "PS5");
    assert!(state.overlay.is_none());
}

#[test]
fn video_wallpaper_mode_keeps_its_setting() {
    let mut preferences = crate::model::Preferences {
        background: "video".into(),
        video_index: 7,
        ..Default::default()
    };
    preferences.sanitize();
    assert_eq!(preferences.background, "video");
    assert_eq!(preferences.video_index, 7);
}

#[test]
fn video_gallery_filters_selection_and_hold_are_independent() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    assert_eq!(
        state.settings_level,
        crate::state::SettingsLevel::WallpaperVideos
    );
    let groups = crate::video_catalog::groups(0);
    assert_eq!(
        controls(&state)
            .iter()
            .filter(|c| matches!(c.target, Target::VideoGroup(_)))
            .count(),
        groups.len()
    );
    let index = groups[0].2[0];
    state.focus = Target::VideoGroup(0);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(
        state.settings_level,
        crate::state::SettingsLevel::WallpaperCategory(0)
    );
    assert_eq!(state.focus, Target::VideoPick(index));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(
        state
            .preferences
            .video_excluded
            .contains(&crate::video_catalog::all()[index].id)
    );
    assert_ne!(state.preferences.background, "video");
    assert_eq!(state.hold(), vec![Effect::Save]);
    assert_eq!(state.preferences.background, "video");
    assert_eq!(state.preferences.video_index, index);
    assert!(
        !state
            .preferences
            .video_excluded
            .contains(&crate::video_catalog::all()[index].id)
    );
    state.back();
    assert_eq!(state.focus, Target::VideoGroup(0));
}
#[test]
fn video_grid_navigation_and_back_to_sources() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    state.focus = Target::VideoGroup(0);
    state.direction(Direction::Right, &controls(&state));
    let items = crate::video_catalog::groups(0)[0].2.clone();
    assert_eq!(state.focus, Target::VideoPick(items[0]));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::VideoPick(items[1]));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::VideoPick(items[5]));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::VideoPick(items[1]));
    state.back();
    assert_eq!(state.focus, Target::VideoGroup(0));
    state.back();
    assert_eq!(state.focus, Target::WallpaperMenu(2));
    state.back();
    assert_eq!(state.focus, Target::Sidebar(1));
    state.back();
    assert_eq!(state.focus, Target::Nav(2));
}

#[test]
fn video_gallery_header_geometry_is_stable_across_source_filters() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open(2);
    state.focus = Target::Sidebar(1);
    state.activate();
    let layout = |state: &State| {
        controls(state)
            .into_iter()
            .filter(|c| {
                matches!(
                    c.target,
                    Target::WallpaperMenu(_) | Target::VideoToggle | Target::VideoShuffle
                )
            })
            .map(|c| (c.target, (c.rect.x, c.rect.y, c.rect.w, c.rect.h)))
            .collect::<Vec<_>>()
    };
    let baseline = layout(&state);
    state.preferences.video_shuffle = !state.preferences.video_shuffle;
    state
        .preferences
        .video_excluded
        .insert(crate::video_catalog::all()[0].id.clone());
    assert_eq!(
        baseline,
        layout(&state),
        "wallpaper layout must not jump with selection changes"
    );
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::VideoGroup(0))
    );
    assert!(
        !controls(&state)
            .iter()
            .any(|c| matches!(c.target, Target::VideoFilter(_)))
    );
}

#[test]
fn video_storage_migrates_old_mask_without_touching_other_fields() {
    let mut prefs = serde_json::to_value(Preferences::default()).unwrap();
    let data = prefs.as_object_mut().unwrap();
    data.remove("video_excluded");
    data.remove("video_shuffle");
    data.remove("video_shuffle_seed");
    data.insert("video_sources_mask".into(), json!(4));
    data.insert("greeting_name".into(), json!("Tester"));
    data.insert("input_names".into(), json!({"com.webos.app.hdmi1":"BEAST"}));
    let path =
        std::env::temp_dir().join(format!("luma-round03-migrate-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&prefs).unwrap()).unwrap();
    let migrated = storage::load(&path).unwrap();
    assert_eq!(
        crate::video_catalog::selected_count(&migrated.video_excluded),
        20
    );
    assert_eq!(migrated.greeting_name, "Tester");
    assert_eq!(
        migrated
            .input_names
            .get("com.webos.app.hdmi1")
            .map(String::as_str),
        Some("BEAST")
    );
    assert!(migrated.video_shuffle);
    storage::save(&path, &migrated).unwrap();
    let roundtrip = storage::load(&path).unwrap();
    assert_eq!(roundtrip.video_excluded, migrated.video_excluded);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn video_handoff_crossfades_selected_still_with_constant_scrim() {
    let mut state = ready();
    state.preferences.background = "video".into();
    state.preferences.video_index = 271;
    state.photo_background_backup = "northern-lights".into();
    state.video_frame_ready = false;
    state.video_transition_started = None;
    let pending = scene(&state);
    assert!(matches!(
        pending.draw.first(),
        Some(crate::renderer::Draw::TintedImage(path, _, _, _, tint))
            if path.ends_with("video-stills/271.jpg") && (tint[3] - 1.).abs() < f32::EPSILON
    ));
    assert!(matches!(
        pending.draw.get(1),
        Some(crate::renderer::Draw::Shape(_, _, _, color))
            if (color[3] - 0.23).abs() < 0.001
    ));

    state.video_frame_ready = true;
    state.video_transition_started = Some(std::time::Instant::now());
    let fading = scene(&state);
    assert!(matches!(
        fading.draw.first(),
        Some(crate::renderer::Draw::TintedImage(path, _, _, _, tint))
            if path.ends_with("video-stills/271.jpg") && tint[3] > 0.9
    ));
    assert!(matches!(
        fading.draw.get(1),
        Some(crate::renderer::Draw::Shape(_, _, _, color))
            if (color[3] - 0.23).abs() < 0.001
    ));

    state.video_transition_started = None;
    let playing = scene(&state);
    assert!(matches!(
        playing.draw.first(),
        Some(crate::renderer::Draw::Shape(_, _, _, color))
            if (color[3] - 0.23).abs() < 0.001
    ));
}

#[test]
fn motion_tabs_preserve_two_layers_on_first_frame_and_retarget_without_stacking() {
    let mut state = ready();
    let assets = view::Assets {
        root: PathBuf::new(),
        version: "test".into(),
        metrics: BTreeMap::new(),
        catalog_metrics: BTreeMap::new(),
        active_heading: std::cell::Cell::new(crate::typography::HeadingFont::default()),
        active_text: std::cell::Cell::new(crate::typography::TextFont::default()),
        heading_pct: std::cell::Cell::new(100),
        text_pct: std::cell::Cell::new(100),
        artwork: crate::app_artwork::Service::new(false),
    };
    let now = time::OffsetDateTime::UNIX_EPOCH;
    let nav = view::navigation_for(&assets, state.preferences.language, false);
    let render = |state: &mut State| {
        let index = state.page_index();
        crate::compose(state, &assets, nav.items[index].x, nav.items[index].w, now)
    };
    let home = render(&mut state);
    let n_home = home.body_range.len();
    assert!(n_home > 0);
    state.open(1);
    let apps = render(&mut state);
    assert_eq!(state.rendered_page, Page::Apps);
    assert!(state.previous_page_layer.is_some());
    assert!(
        apps.body_range.len() > state.previous_page_layer.as_ref().unwrap().0.len(),
        "first page-switch frame contains both outgoing and incoming draws"
    );
    state.open(2);
    let settings = render(&mut state);
    assert_eq!(state.rendered_page, Page::Settings);
    assert!(!settings.body_range.is_empty());
    assert_eq!(
        state.previous_page_layer.as_ref().unwrap().0.len(),
        apps.body_range.len(),
        "fast double-navigation targets the last composited visible frame"
    );
}

#[test]
fn remote_scroll_animates_and_pointer_control_follows_visible_card() {
    let mut state = ready();
    state.open(1);
    let last = state.apps().last().unwrap().id.clone();
    state.focus(Target::App(last.clone()), &controls(&state));
    let desired = state.scroll_goal;
    assert!(desired > 0.);
    let previous = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::App(last.clone()))
        .unwrap()
        .rect
        .y;
    state.scroll_last = std::time::Instant::now() - std::time::Duration::from_millis(16);
    state.tick();
    assert!(state.scroll > 0. && state.scroll < desired);
    let current = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::App(last.clone()))
        .unwrap()
        .rect
        .y;
    assert!(
        current < previous,
        "pointer hit rectangle tracks native drawn scroll position"
    );
}

#[test]
fn focus_retarget_resumes_from_current_intensity_without_reset() {
    let mut state = ready();
    let a = Target::Sidebar(0);
    let b = Target::Sidebar(1);
    state.change_focus(a.clone());
    state.focus_tweens.get_mut(&a).unwrap().start -= std::time::Duration::from_millis(70);
    let before = state.focus_level(&a);
    assert!(before > 0. && before < 1.);
    state.change_focus(b.clone());
    assert!((state.focus_tweens[&a].from - before).abs() < 0.01);
    assert_eq!(state.focus_tweens[&b].to, 1.);
}

#[test]
fn rude_greeting_is_default_and_choice_persists_without_touching_other_preferences() {
    let mut state = ready();
    assert_eq!(
        state.preferences.greeting_style,
        crate::weather_phrases::GreetingStyle::Rude
    );
    state.open(2);
    state.focus = Target::Setting(6);
    state.activate();
    assert_eq!(state.focus, Target::GreetingStyle);
    state.activate();
    assert_eq!(state.subpage, Some(crate::state::Subpage::GreetingStyle));
    assert_eq!(state.focus, Target::Choice(1));
    state.focus = Target::Choice(0);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.greeting_style,
        crate::weather_phrases::GreetingStyle::Classic
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Greeting));
    assert_eq!(state.focus, Target::GreetingStyle);
    let prefs = serde_json::to_string(&state.preferences).unwrap();
    let roundtrip: crate::model::Preferences = serde_json::from_str(&prefs).unwrap();
    assert_eq!(
        roundtrip.greeting_style,
        crate::weather_phrases::GreetingStyle::Classic
    );
    state.back();
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(6));
    let legacy: crate::model::Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(
        legacy.greeting_style,
        crate::weather_phrases::GreetingStyle::Rude
    );
}

#[test]
fn smart_home_preferences_backward_compatible() {
    let old = serde_json::json!({"background":"dunes","favorites":["spotify-beehive"]});
    let settings: crate::model::Preferences = serde_json::from_value(old).unwrap();
    assert!(!settings.smart_home.show_tab);
    assert!(settings.smart_home.server_url.is_empty());
    let value = serde_json::to_value(&settings).unwrap();
    let reread: crate::model::Preferences = serde_json::from_value(value).unwrap();
    assert!(!reread.smart_home.show_tab);
    assert_eq!(reread.favorites.as_ref().unwrap()[0], "spotify-beehive");
}
#[test]
fn smart_home_tab_visibility_and_settings_index() {
    let mut state = crate::state::State::new(crate::model::Preferences::default());
    assert_eq!(state.settings_nav_index(), 2);
    state.open(2);
    assert_eq!(state.page, crate::state::Page::Settings);
    state.preferences.smart_home.server_url = "http://homeassistant.local:8123".into();
    state.preferences.smart_home.show_tab = true;
    assert_eq!(state.settings_nav_index(), 3);
    state.open(2);
    assert_eq!(state.page, crate::state::Page::SmartHome);
    state.open(3);
    assert_eq!(state.page, crate::state::Page::Settings);
}

#[test]
fn smart_home_tvos_push_pop_and_selection() {
    use crate::state::{Page, SMARTHOME_SECTION, Subpage, Target};
    let mut state = crate::state::State::new(crate::model::Preferences::default());
    state.preferences.smart_home.server_url = "http://homeassistant.local:8123".into();
    state.preferences.smart_home.show_tab = true;
    state.ha_entities = vec![
        crate::smart_home::Entity {
            id: "light.office".into(),
            name: "Office".into(),
            state: "on".into(),
            domain: "light".into(),
            area: "Office".into(),
            brightness: Some(50),
            unit: String::new(),
            hidden: false,
            device_class: String::new(),
        },
        crate::smart_home::Entity {
            id: "light.living".into(),
            name: "Living".into(),
            state: "off".into(),
            domain: "light".into(),
            area: "Living Room".into(),
            brightness: None,
            unit: String::new(),
            hidden: false,
            device_class: String::new(),
        },
    ];
    state.open_settings();
    state.section = SMARTHOME_SECTION;
    state.focus = Target::SmartSection(0);
    state.activate();
    assert_eq!(state.page, Page::Settings);
    assert_eq!(state.subpage, Some(Subpage::SmartSelect(0)));
    assert_eq!(state.focus, Target::SmartBulk(0, 0));
    state.focus = Target::SmartPick(0, "light.living".into());
    state.activate();
    assert_eq!(
        state.preferences.smart_home.selected_entities,
        vec!["light.office"]
    );
    state.back();
    assert_eq!(state.subpage, None);
    assert_eq!(state.focus, Target::SmartSection(0));
}

#[test]
fn smart_home_selection_explicitly_none_is_not_default_all() {
    use crate::state::Target;
    let mut state = crate::state::State::new(crate::model::Preferences::default());
    state.ha_entities = vec![crate::smart_home::Entity {
        id: "camera.front".into(),
        name: "Front".into(),
        state: "idle".into(),
        domain: "camera".into(),
        area: "".into(),
        brightness: None,
        unit: String::new(),
        hidden: false,
        device_class: String::new(),
    }];
    state.focus = Target::SmartPick(1, "camera.front".into());
    state.activate();
    assert_eq!(
        state.preferences.smart_home.selected_cameras,
        vec!["__none__"]
    );
    state.activate();
    assert_eq!(
        state.preferences.smart_home.selected_cameras,
        vec!["camera.front"]
    );
}

#[test]
fn tvos_settings_push_pop_reveals_parent_focus_without_traps() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.open_settings();
    assert_eq!(state.settings_level, SettingsLevel::Root);
    assert_eq!(state.focus, Target::Nav(2));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(0));
    state.focus = Target::Sidebar(1);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::WallpaperMenu(0));
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::WallpaperVideos);
    state.focus = Target::VideoGroup(0);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.settings_level, SettingsLevel::WallpaperCategory(0));
    assert!(matches!(state.focus, Target::VideoPick(_)));
    state.back();
    assert_eq!(state.focus, Target::VideoGroup(0));
    state.back();
    assert_eq!(state.focus, Target::WallpaperMenu(2));
    state.back();
    assert_eq!(state.focus, Target::Sidebar(1));
    state.back();
    assert_eq!(state.focus, Target::Nav(2));
    state.back();
    assert_eq!(state.page, Page::Home);
}

#[test]
fn tvos_settings_scene_never_uses_small_body_text() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.open_settings();
    let sections = [0, 1, 2, 3, 4, 5];
    for section in sections {
        state.section = section;
        state.settings_level = SettingsLevel::Section;
        let rendered = scene(&state);
        for draw in &rendered.draw[rendered.settings_panel_range.clone()] {
            if let crate::renderer::Draw::Text(label, _, style, _, _) = draw {
                assert!(
                    style.size >= 25.,
                    "too small for TV: {label} @ {}",
                    style.size
                );
            }
        }
    }
    for level in [
        SettingsLevel::Root,
        SettingsLevel::WallpaperType,
        SettingsLevel::WallpaperPhotos,
        SettingsLevel::WallpaperVideos,
        SettingsLevel::WallpaperCategory(0),
    ] {
        state.section = 1;
        state.settings_level = level;
        let rendered = scene(&state);
        for draw in &rendered.draw[rendered.settings_panel_range.clone()] {
            if let crate::renderer::Draw::Text(label, _, style, _, _) = draw {
                assert!(
                    style.size >= 25.,
                    "too small for TV: {label} @ {}",
                    style.size
                );
            }
        }
    }
}

#[test]
fn tvos_category_toggle_keeps_prefs_and_source_selection() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    let favorite = "spotify-beehive".to_owned();
    state.preferences.favorites = Some(vec![favorite.clone()]);
    state.preferences.greeting_name = "Living room".into();
    state
        .preferences
        .input_names
        .insert("hdmi1".into(), "BEAST".into());
    state.open_settings();
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::WallpaperVideos);
    let count = state.video_selected_count();
    state.focus = Target::VideoSource(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.video_selected_count() < count);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.video_selected_count(), count);
    state.focus = Target::VideoGroup(0);
    let before = state.preferences.video_excluded.clone();
    assert!(state.activate().is_empty());
    assert_eq!(state.settings_level, SettingsLevel::WallpaperCategory(0));
    assert_eq!(state.preferences.video_excluded, before);
    let serialized = serde_json::to_string(&state.preferences).unwrap();
    let restored: Preferences = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.favorites.unwrap(), vec![favorite]);
    assert_eq!(restored.greeting_name, "Living room");
    assert_eq!(
        restored.input_names.get("hdmi1").map(String::as_str),
        Some("BEAST")
    );
}

#[test]
fn tvos_legacy_section_body_clears_page_heading() {
    use crate::state::{INPUTS_SECTION, SettingsLevel, WEATHER_SECTION};
    let mut state = ready();
    state.open_settings();
    state.settings_level = SettingsLevel::Section;
    for (section, target) in [
        (0, Target::Setting(0)),
        (WEATHER_SECTION, Target::WeatherToggle),
        (
            INPUTS_SECTION,
            Target::InputMode("com.webos.app.hdmi1".into()),
        ),
        (5, Target::Setting(3)),
    ] {
        state.section = section;
        let screen = scene(&state);
        let first = screen
            .controls
            .iter()
            .find(|c| c.target == target)
            .expect("first setting should be rendered");
        assert!(
            first.rect.y >= 184.,
            "row overlaps title in section {section}: {}",
            first.rect.y
        );
    }
}

#[test]
fn tvos_magic_remote_chevron_opens_category_without_mutating_selection() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.open_settings();
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    let before = state.preferences.video_excluded.clone();
    let screen = scene(&state);
    assert!(
        screen
            .controls
            .iter()
            .any(|c| c.target == Target::VideoGroup(0))
    );
    state.pointer_focus(Target::VideoGroup(0));
    assert!(state.activate().is_empty());
    assert_eq!(state.settings_level, SettingsLevel::WallpaperCategory(0));
    assert_eq!(state.preferences.video_excluded, before);
}

#[test]
fn settings_v2_pointer_back_at_each_depth_returns_to_parent() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    state.focus = Target::VideoGroup(0);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.settings_level, SettingsLevel::WallpaperCategory(0));
    for (expected, focus) in [
        (SettingsLevel::WallpaperVideos, Target::VideoGroup(0)),
        (SettingsLevel::Section, Target::WallpaperMenu(2)),
        (SettingsLevel::Root, Target::Sidebar(1)),
    ] {
        let scene = scene(&state);
        let arrow = scene
            .controls
            .iter()
            .find(|c| c.target == Target::SettingsBack)
            .expect("pointer back arrow has clickable control");
        assert_eq!(arrow.rect.w, 56.);
        assert_eq!(arrow.rect.h, 56.);
        assert_eq!(arrow.rect.x, 108.);
        assert_eq!(arrow.rect.y, 208.);
        state.pointer_focus(Target::SettingsBack);
        state.activate();
        assert_eq!(state.settings_level, expected);
        assert_eq!(state.focus, focus);
    }
    assert!(
        scene(&state)
            .controls
            .iter()
            .all(|c| c.target != Target::SettingsBack)
    );
}
#[test]
fn settings_v2_glass_rows_are_exactly_80px_with_no_left_row_icons() {
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = true;
    let layout = scene(&state);
    let rows: Vec<_> = layout
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::Sidebar(_)))
        .collect();
    assert_eq!(rows.len(), 8);
    assert!(
        rows.iter()
            .all(|r| r.rect.h == 80. && r.rect.x == 864. && r.rect.w == 940.)
    );
    assert!(layout.draw.iter().any(
        |d| matches!(d,crate::renderer::Draw::Shape(r, radius, _,color)
      if r.x==832. && *radius==28. && color[3] > 0.3)
    ));
}

#[test]
fn settings_v2_category_poster_pitch_keeps_captions_clear() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.open_settings();
    state.settings_level = SettingsLevel::WallpaperCategory(0);
    let scene = scene(&state);
    let cards: Vec<_> = scene
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::VideoPick(_)))
        .collect();
    assert!(cards.len() >= 8);
    assert!((cards[4].rect.y - cards[0].rect.y - 278.).abs() < 0.01);
    assert_eq!(cards[0].rect.y, cards[1].rect.y);
}

#[test]
fn settings_v2_smart_connect_preserves_parent_and_has_no_token_control() {
    use crate::state::{SettingsLevel, Subpage};
    let mut state = ready();
    state.open_settings();
    state.focus = Target::Sidebar(crate::state::SMARTHOME_SECTION);
    state.activate();
    state.focus = Target::SmartConnect;
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.subpage, Some(Subpage::SmartConnect));
    let screen = scene(&state);
    assert!(
        screen
            .controls
            .iter()
            .any(|c| c.target == Target::SmartRetry)
    );
    assert!(
        screen
            .draw
            .iter()
            .any(|d| matches!(d, crate::renderer::Draw::Text(label,_,_,_,_)
        if label == "SIGN IN WITH YOUR PHONE"))
    );
    // The token path is a second phone QR, never remote typing.
    assert!(
        screen
            .controls
            .iter()
            .any(|c| c.target == Target::SmartFallback)
    );
    state.pointer_focus(Target::SettingsBack);
    state.activate();
    assert_eq!(state.subpage, None);
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::SmartConnect);
}

#[test]
fn settings_v2_up_navigation_reaches_back_without_leaving_page() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.open_settings();
    state.focus = Target::Sidebar(1);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::SettingsBack);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Root);
    assert_eq!(state.focus, Target::Sidebar(1));
}

#[test]
fn settings_revision_back_reaches_home_and_nav_home_click_works_at_any_depth() {
    use crate::state::{Page, SettingsLevel};
    for level in [
        SettingsLevel::Root,
        SettingsLevel::Section,
        SettingsLevel::WallpaperVideos,
        SettingsLevel::WallpaperCategory(0),
    ] {
        let mut state = ready();
        state.open_settings();
        state.section = 1;
        state.settings_level = level;
        state.focus = match level {
            SettingsLevel::Root => Target::Sidebar(1),
            SettingsLevel::Section => Target::WallpaperMenu(2),
            SettingsLevel::WallpaperVideos => Target::VideoGroup(0),
            SettingsLevel::WallpaperCategory(_) => Target::VideoPick(0),
            _ => unreachable!(),
        };
        let mut steps = 0;
        while state.page == Page::Settings && steps < 9 {
            state.back();
            steps += 1;
        }
        assert_eq!(
            state.page,
            Page::Home,
            "Back from {level:?} must reach Home"
        );
        assert!(steps <= 5);
        let mut state = ready();
        state.open_settings();
        state.section = 1;
        state.settings_level = level;
        state.pointer_focus(Target::Nav(0));
        state.activate();
        assert_eq!(
            state.page,
            Page::Home,
            "Home pointer navigation from {level:?}"
        );
    }
}

#[test]
fn settings_revision_panels_share_top_height_and_no_extra_headings() {
    use crate::state::SettingsLevel;
    for (level, section) in [
        (SettingsLevel::Root, 0),
        (SettingsLevel::Section, 0),
        (SettingsLevel::Section, 1),
        (SettingsLevel::WallpaperVideos, 1),
    ] {
        let mut state = ready();
        state.open_settings();
        state.settings_level = level;
        state.section = section;
        state.preferences.reduced_motion = true;
        let rendered = scene(&state);
        let panels: Vec<_> = rendered
            .draw
            .iter()
            .filter_map(|d| {
                if let crate::renderer::Draw::Shape(r, rad, _, col) = d
                    && *rad == 28.
                    && r.y == 184.
                    && col[3] > 0.3
                {
                    return Some(r);
                }
                None
            })
            .collect();
        assert_eq!(panels.len(), 2, "{level:?}/{section} glass panels");
        assert_eq!(panels[0].h, panels[1].h);
        assert_eq!(panels[0].x, crate::state::HOME_STRIP_LEFT);
        assert_eq!(panels[1].x, 832.);
        assert_eq!(panels[1].x + panels[1].w, crate::state::HOME_STRIP_RIGHT);
        assert!(
            rendered.draw[rendered.settings_panel_range.clone()]
                .iter()
                .all(|d| !matches!(d,crate::renderer::Draw::Text(t,_,_,_,_)
            if (t=="Wallpaper"||t=="Settings") && level != SettingsLevel::Root))
        );
    }
}

#[test]
fn settings_preview_illustrations_stay_in_panel_with_centered_copy() {
    use crate::renderer::Draw;
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = true;
    let mut keys = Vec::new();
    for (section, name, summary) in [
        (0, "General", "Language, clock and motion"),
        (1, "Wallpaper", "Video wallpaper collection"),
        (2, "Widgets", "Weather, crypto, HDMI and Smart Home on Home"),
        (3, "Inputs", "Connected HDMI devices"),
        (4, "Smart Home", "Home Assistant devices"),
        (5, "System", "Device and launcher controls"),
        (6, "Appearance", "Accent color and highlights"),
        (7, "Stacks", "Rows of media on Home"),
    ] {
        state.focus = Target::Sidebar(section);
        let layout = scene(&state);
        // Illustrations replaced the line icons.
        assert!(!layout.draw.iter().any(|d| matches!(
            d,
            Draw::TintedImage(path, rect, _, _, _) if path.contains("ui-icons/") && rect.w == 144.
        )));
        // The same preview keeps one identity between frames, so moving
        // focus within it never crossfades a scene onto itself.
        assert_ne!(layout.settings_sidebar_key, 0);
        assert_eq!(
            layout.settings_sidebar_key,
            scene(&state).settings_sidebar_key
        );
        keys.push(layout.settings_sidebar_key);
        let center_x = 84. + 700. / 2.;
        for label in [name, summary] {
            let (rect, centered) = layout
                .draw
                .iter()
                .find_map(|d| match d {
                    Draw::Text(text, rect, _, _, centered) if text == label => {
                        Some((rect, centered))
                    }
                    _ => None,
                })
                .expect("preview copy");
            assert!(*centered);
            assert!((rect.x + rect.w / 2. - center_x).abs() < 0.01);
        }
        // Every visible shape of the scene stays inside the preview glass.
        let mut clip: Option<crate::state::Rect> = None;
        for d in &layout.draw[layout.settings_sidebar_range.clone()] {
            if let Draw::Clip(c) = d {
                clip = *c;
            }
            if let Draw::Shape(r, ..)
            | Draw::DiagonalGradient(r, ..)
            | Draw::RoundedGradient(r, ..) = d
            {
                let (left, right) = clip.map_or((r.x, r.x + r.w), |c| {
                    (r.x.max(c.x), (r.x + r.w).min(c.x + c.w))
                });
                if right <= left {
                    continue;
                }
                assert!(left >= 84. && right <= 784., "{name}: {r:?}");
                assert!(r.y >= 184. && r.y + r.h <= 184. + 788., "{name}: {r:?}");
            }
        }
    }
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 8);
}

#[test]
fn settings_v2_back_is_within_left_glass_panel() {
    let mut state = ready();
    state.open_settings();
    state.focus = Target::Sidebar(0);
    state.activate();
    state.preferences.reduced_motion = true;
    let layout = scene(&state);
    let button = layout
        .controls
        .iter()
        .find(|c| c.target == Target::SettingsBack)
        .unwrap();
    assert_eq!(
        (button.rect.x, button.rect.y, button.rect.w, button.rect.h),
        (108., 208., 56., 56.)
    );
    assert!(button.rect.x >= 84. + 24. && button.rect.y >= 184. + 24.);
    assert!(button.rect.x + button.rect.w <= 84. + 700. - 24.);
    assert!(button.rect.y + button.rect.h <= 184. + 568. - 24.);
}

#[test]
fn settings_v2_back_does_not_cover_wallpaper_thumbnails_or_category_count() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.section = 1;
    state.settings_level = SettingsLevel::Section;
    let frame = scene(&state);
    let back = frame
        .controls
        .iter()
        .find(|c| c.target == Target::SettingsBack)
        .unwrap()
        .rect;
    let images: Vec<_> = frame
        .draw
        .iter()
        .filter_map(|d| match d {
            crate::renderer::Draw::Image(path, rect, _, _)
                if path.contains("video-thumbnails/") =>
            {
                Some(rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 6);
    assert!(images.iter().all(|r| r.y >= back.y + back.h + 8.));
    state.settings_level = SettingsLevel::WallpaperCategory(0);
    let frame = scene(&state);
    let back = frame
        .controls
        .iter()
        .find(|c| c.target == Target::SettingsBack)
        .unwrap()
        .rect;
    let count = frame
        .draw
        .iter()
        .find_map(|d| match d {
            crate::renderer::Draw::Text(label, rect, _, _, _) if label == "23 of 23 selected" => {
                Some(rect)
            }
            _ => None,
        })
        .unwrap();
    assert!(count.x >= back.x + back.w + 8.);
}

#[test]
fn settings_revision_select_all_is_56px_centered_and_inside_panel() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.settings_level = SettingsLevel::WallpaperCategory(0);
    let frame = scene(&state);
    let select = frame
        .controls
        .iter()
        .find(|c| c.target == Target::VideoSelectAll(0))
        .expect("Select All button")
        .rect;
    assert_eq!(select.y, 208.);
    assert_eq!(select.h, 56.);
    assert!(select.y + select.h <= 992.);
    let text = frame
        .draw
        .iter()
        .find_map(|d| match d {
            crate::renderer::Draw::Text(s, r, _, _, center)
                if s == "Deselect All" || s == "Select All" =>
            {
                assert!(*center);
                Some(r)
            }
            _ => None,
        })
        .expect("Select All label");
    assert!((text.y + text.h / 2. - (select.y + select.h / 2.)).abs() <= 2.);
}

#[test]
fn settings_inputs_group_connected_first_without_phantom_gap() {
    let mut state = ready();
    state.open_settings();
    state.section = crate::state::INPUTS_SECTION;
    state.settings_level = crate::state::SettingsLevel::Section;
    state.preferences.reduced_motion = true;
    let layout = scene(&state);
    let mut rows = layout
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::InputMode(_)))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    assert_eq!(rows.len(), state.inventory.inputs.len());
    assert!(
        rows.iter()
            .all(|r| r.rect.h == 104. && r.rect.x == 864. && r.rect.w == 940.)
    );
    assert!(layout.draw.iter().any(|d| matches!(d,
        crate::renderer::Draw::Text(text,_,_,_,_) if text=="CONNECTED")));
}

#[test]
fn settings_shared_horizontal_edges_match_home_and_apps() {
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = true;
    let rendered = scene(&state);
    let glass = rendered
        .draw
        .iter()
        .filter_map(|d| {
            if let crate::renderer::Draw::Shape(r, rad, _, color) = d
                && *rad == 28.
                && r.y == 184.
                && color[3] > 0.3
            {
                return Some(*r);
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(glass.len(), 2);
    assert_eq!(glass[0].x, crate::state::HOME_STRIP_LEFT);
    assert_eq!(glass[1].x + glass[1].w, crate::state::HOME_STRIP_RIGHT);
}
#[test]
fn video_sources_and_categories_have_distinct_pointer_hitboxes() {
    use crate::state::SettingsLevel;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.focus = Target::Sidebar(1);
    state.activate();
    state.focus = Target::WallpaperMenu(2);
    state.activate();
    let controls = scene(&state).controls;
    assert!(controls.iter().any(|r| r.target == Target::VideoSource(1)));
    assert!(controls.iter().any(|r| r.target == Target::VideoGroup(0)));
    let before = state.video_selected_count();
    state.pointer_focus(Target::VideoSource(1));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.video_selected_count() < before);
    state.pointer_focus(Target::VideoGroup(0));
    assert!(state.activate().is_empty());
    assert_eq!(state.settings_level, SettingsLevel::WallpaperCategory(0));
    let clip = crate::video_catalog::groups(0)[0].2[0];
    state.focus = Target::VideoPick(clip);
    let before_clip = state
        .preferences
        .video_excluded
        .contains(&crate::video_catalog::all()[clip].id);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_ne!(
        before_clip,
        state
            .preferences
            .video_excluded
            .contains(&crate::video_catalog::all()[clip].id)
    );
}

#[test]
fn settings_root_six_rows_have_symmetric_32px_inner_padding() {
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = true;
    let layout = scene(&state);
    let panels = layout
        .draw
        .iter()
        .filter_map(crate::view::tvos_settings_view::backdrop_rect)
        .collect::<Vec<_>>();
    assert_eq!(panels.len(), 6);
    assert!(panels.iter().all(|r| (r.h - 788.).abs() < 0.01));
    let last = layout
        .controls
        .iter()
        .find(|c| c.target == Target::Sidebar(5))
        .unwrap();
    assert!((last.rect.y + last.rect.h + 32. - (184. + 788.)).abs() < 0.01);
}
#[test]
fn settings_backdrops_stay_in_place_during_content_cascade_and_height_change() {
    use crate::state::SettingsLevel;
    use std::time::{Duration, Instant};
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = false;
    let old_scene = scene(&state);
    state.last_body = old_scene.draw[old_scene.body_range.clone()].to_vec();
    state.settings_level = SettingsLevel::Section;
    state.section = 0;
    let old = old_scene.draw[old_scene.settings_panel_range.clone()].to_vec();
    state.previous_panel_layer = Some((old, Instant::now() - Duration::from_millis(100), true));
    state.panel_started = Some(Instant::now() - Duration::from_millis(100));
    let changed = scene(&state);
    let panels = changed.draw[changed.settings_panel_range.clone()]
        .iter()
        .filter_map(crate::view::tvos_settings_view::backdrop_rect)
        .collect::<Vec<_>>();
    assert_eq!(panels.len(), 6);
    for r in panels {
        assert!(r.y == 184. || r.y == 192.);
        assert!(r.x == crate::state::HOME_STRIP_LEFT || r.x == 832.);
        assert!(
            r.h < 696. && r.h > 568.,
            "height should ease between pages: {}",
            r.h
        );
    }
    let background_count = changed
        .draw
        .iter()
        .filter_map(crate::view::tvos_settings_view::backdrop_rect)
        .count();
    assert_eq!(background_count, 6);
}
#[test]
fn focused_connected_input_keeps_prefix_readable_and_green_status() {
    let mut state = ready();
    state.open_settings();
    state.section = crate::state::INPUTS_SECTION;
    state.settings_level = crate::state::SettingsLevel::Section;
    state.preferences.reduced_motion = true;
    state.inventory.inputs[0].title = "PC".into();
    state.focus = Target::InputMode(state.inventory.inputs[0].id.clone());
    let layout = scene(&state);
    let mut found_prefix = false;
    let mut found_green = false;
    for d in &layout.draw {
        if let crate::renderer::Draw::Text(value, rect, _, color, _) = d {
            if value == "PC · " {
                found_prefix = true;
                assert_eq!(rect.x, 896.);
                assert!(color[0] < 0.4 && color[1] < 0.4 && color[2] < 0.4);
            }
            if value == "Connected" {
                found_green = true;
                assert!(
                    (color[0] - 48. / 255.).abs() < 0.01 && (color[1] - 209. / 255.).abs() < 0.01
                );
            }
        }
    }
    assert!(found_prefix && found_green);
}

#[test]
fn accent_preferences_default_is_backwards_compatible_and_persists() {
    let mut prefs = Preferences::default();
    // The default look is the default: Citrus gradient everywhere.
    assert_eq!(prefs.accent.id, "Citrus");
    assert_eq!(prefs.accent.kind, "gradient");
    assert!(prefs.accent.use_focus && prefs.accent.use_nav && prefs.accent.use_headline);
    prefs.accent.choose(8);
    assert_eq!(prefs.accent.id, "Aurora");
    prefs.accent.use_focus = true;
    prefs.accent.use_nav = true;
    prefs.accent.use_headline = true;
    let stored = serde_json::to_string(&prefs).unwrap();
    assert!(stored.contains("\"accent\""));
    let loaded: Preferences = serde_json::from_str(&stored).unwrap();
    assert_eq!(loaded.accent.id, "Aurora");
    assert_eq!(loaded.accent.colors(), (0xA978FF, 0x3FA2FF));
    assert!(loaded.accent.use_focus && loaded.accent.use_nav && loaded.accent.use_headline);
    let mut old = serde_json::to_value(prefs).unwrap();
    old.as_object_mut().unwrap().remove("accent");
    let migrated: Preferences = serde_json::from_value(old).unwrap();
    assert_eq!(migrated.accent.id, "Citrus");
    assert!(migrated.accent.use_focus);
}
#[test]
fn appearance_has_four_eight_color_rows_and_three_toggles() {
    let mut state = ready();
    legacy_look(&mut state.preferences);
    state.open_settings();
    state.section = crate::state::APPEARANCE_SECTION;
    state.settings_level = crate::state::SettingsLevel::Section;
    state.focus = Target::AccentChoice(7);
    state.preferences.reduced_motion = true;
    let rendered = scene(&state);
    assert_eq!(
        rendered
            .controls
            .iter()
            .filter(|r| matches!(r.target, Target::AccentChoice(_)))
            .count(),
        16
    );
    assert_eq!(
        rendered
            .controls
            .iter()
            .filter(|r| matches!(r.target, Target::AccentOption(_)))
            .count(),
        3
    );
    assert_eq!(
        rendered
            .draw
            .iter()
            .filter(|d| matches!(d, crate::renderer::Draw::DiagonalGradient(_, _, _, _)))
            .count(),
        8
    );
    state.focus = Target::AccentChoice(8);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.accent.id, "Aurora");
    state.focus = Target::AccentOption(0);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.accent.use_focus);
}
#[test]
fn appearance_navigation_preserves_root_position_and_back() {
    let mut state = ready();
    state.open_settings();
    state.preferences.reduced_motion = true;
    state.focus = Target::Sidebar(1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(
        state.focus,
        Target::Sidebar(crate::state::APPEARANCE_SECTION)
    );
    state.activate();
    assert_eq!(state.focus, Target::AccentChoice(7));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::AccentChoice(15));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::AccentOption(0));
    state.back();
    assert_eq!(
        state.focus,
        Target::Sidebar(crate::state::APPEARANCE_SECTION)
    );
}

#[test]
fn appearance_left_direction_never_navigates_back() {
    use crate::state::{APPEARANCE_SECTION, SettingsLevel};
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.section = APPEARANCE_SECTION;
    state.settings_level = SettingsLevel::Section;
    state.focus = Target::AccentChoice(7);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::AccentChoice(6));
    state.focus = Target::AccentChoice(0);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::SettingsBack);
    state.focus = Target::AccentChoice(8);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::SettingsBack);
    state.focus = Target::AccentOption(0);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::SettingsBack);
    state.focus = Target::SettingsBack;
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Root);
    assert_eq!(state.focus, Target::Sidebar(APPEARANCE_SECTION));
}
#[test]
fn bright_accent_navigation_uses_dark_ink() {
    let dark = crate::view::contrast_text_for_accent(0xC6FF00, 0x00D5FF);
    assert!(dark[0] < 0.15 && dark[1] < 0.15 && dark[2] < 0.15);
    let bright = crate::view::contrast_text_for_accent(0xFFFFFF, 0xFFFFFF);
    assert!(bright[0] < 0.15);
    let pale_blue = crate::view::contrast_text_for_accent(0x64D2FF, 0x0A84FF);
    assert!(pale_blue[0] < 0.15);
}
#[test]
fn gradient_focus_ring_uses_two_color_native_border() {
    use crate::state::{APPEARANCE_SECTION, SettingsLevel};
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.preferences.accent.choose(12); // 8 solids + Citrus gradient index 4
    state.preferences.accent.use_focus = true;
    state.open_settings();
    state.focus = Target::Sidebar(APPEARANCE_SECTION);
    let rendered = scene(&state);
    // The focused Settings row is one solid accent gradient fill (no outline).
    assert!(
        rendered
            .draw
            .iter()
            .any(|d| matches!(d, crate::renderer::Draw::DiagonalGradient(_, _, _, _)))
    );
    assert_eq!(state.preferences.accent.colors(), (0xFFE94A, 0x3FD3FF));
    state.settings_level = SettingsLevel::Section;
    state.section = APPEARANCE_SECTION;
    let scene = scene(&state);
    let swatches = scene
        .controls
        .iter()
        .filter(|r| matches!(r.target, Target::AccentChoice(_)))
        .count();
    assert_eq!(swatches, 16);
}

#[test]
fn typography_all_pages_use_semantic_tv_scale() {
    use crate::renderer::Draw;
    let scale = [25., 29., 31., 38., 52., 108.];
    let mut state = ready();
    legacy_look(&mut state.preferences);
    for page in 0..3 {
        state.open(page);
        let scene = scene(&state);
        let mut seen = 0;
        // Settings illustrations scale their own fixed-size type with the scene.
        let art = if scene.settings_sidebar_key != 0 {
            scene.settings_sidebar_range.clone()
        } else {
            0..0
        };
        for (index, command) in scene.draw.into_iter().enumerate() {
            if art.contains(&index) {
                continue;
            }
            if let Draw::Text(_, rect, style, _, _) = command {
                // Home widgets are deliberately denser TV type: their 18–84px
                // hierarchy keeps six hourly columns and price rows whole.
                // The zone is grid columns 4–6 with the text line above it.
                let weather = page == 0
                    && rect.x >= crate::state::HOME_STRIP_LEFT + 3. * crate::state::HOME_TILE_PITCH
                    && rect.y >= 168. - state.scroll
                    && rect.y <= 640. - state.scroll;
                // The caption under a focused header circle is deliberately small.
                let circle_caption = rect.y >= 112. && rect.y < 160. && rect.x > 1200.;
                if circle_caption {
                    assert!(style.size >= 19.);
                    continue;
                }
                // Header clock (as tall as the navigation pill) and the date
                // beside it, which steps down to fit before the navigation.
                if rect.y < 112. && rect.x < 760. {
                    assert!(style.size >= 21.);
                    continue;
                }
                if weather {
                    assert!(
                        (18. ..=84.).contains(&style.size),
                        "weather text must stay legible: {}",
                        style.size
                    );
                } else {
                    // Small labels keep their own size from the 18 px floor.
                    assert!(
                        scale.contains(&style.size) || (18. ..27.).contains(&style.size),
                        "page {page}: {}",
                        style.size
                    );
                }
                assert!(style.weight >= 500, "page {page}: {}", style.weight);
                seen += 1;
            }
        }
        assert!(seen > 0, "page {page} contains no text");
    }
}

#[test]
fn independent_fonts_roundtrip_preserves_preferences_and_limits() {
    use crate::typography::{HeadingFont, TextFont};
    let mut original = serde_json::to_value(Preferences::default()).unwrap();
    original["heading_font"] = json!("unbounded");
    original["text_font"] = json!("golostext");
    original["heading_size_pct"] = json!(110);
    original["text_size_pct"] = json!(90);
    original["hidden"] = json!(["com.example.hidden"]);
    original["input_names"] = json!({"hdmi1":"BEAST","hdmi2":"Apple TV"});
    let p: Preferences = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(p.heading_font, HeadingFont::Unbounded);
    assert_eq!(p.text_font, TextFont::Golostext);
    assert_eq!(p.heading_size_pct, 110);
    let saved = serde_json::to_value(p).unwrap();
    assert_eq!(saved["hidden"], original["hidden"]);
    assert_eq!(saved["input_names"], original["input_names"]);
    assert_eq!(saved["heading_font"], "unbounded");
    assert_eq!(saved["text_font"], "golostext");
}
#[test]
fn appearance_has_independent_font_lists_and_clipped_scroll() {
    use crate::state::{APPEARANCE_SECTION, SettingsLevel};
    use crate::typography::{HeadingFont, TextFont};
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.focus = Target::Sidebar(APPEARANCE_SECTION);
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    let rendered = scene(&state);
    assert!(
        rendered
            .controls
            .iter()
            .any(|c| c.target == Target::HeadingFontOpen)
    );
    assert!(
        rendered
            .controls
            .iter()
            .any(|c| c.target == Target::TextFontOpen)
    );
    assert!(
        rendered
            .controls
            .iter()
            .any(|c| c.target == Target::HeadingSize)
    );
    assert!(
        rendered
            .controls
            .iter()
            .any(|c| c.target == Target::TextSize)
    );
    assert_eq!(
        rendered
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::AccentChoice(_)))
            .count(),
        16
    );
    state.focus = Target::HeadingFontOpen;
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::HeadingFonts);
    let rendered = scene(&state);
    assert_eq!(
        rendered
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::HeadingFontPick(_)))
            .count(),
        HeadingFont::ALL.len()
    );
    state.focus = Target::HeadingFontPick(2);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.heading_font, HeadingFont::Unbounded);
    state.back();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::HeadingFontOpen);
    state.focus = Target::TextFontOpen;
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::TextFonts);
    assert_eq!(
        scene(&state)
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::TextFontPick(_)))
            .count(),
        TextFont::ALL.len()
    );
    state.focus = Target::TextFontPick(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.text_font, TextFont::Manrope);
    state.back();
    assert_eq!(state.focus, Target::TextFontOpen);
    state.focus = Target::HeadingSize;
    state.preferences.heading_size_pct = 170;
    state.activate();
    assert_eq!(state.preferences.heading_size_pct, 85);
    state.focus = Target::TextSize;
    state.preferences.text_size_pct = 150;
    state.activate();
    assert_eq!(state.preferences.text_size_pct, 90);
}
#[test]
fn appearance_approved_2240_palette_and_spacing() {
    use crate::model::{ACCENT_GRADIENTS, ACCENT_SOLIDS};
    let reference_solids = [
        0x4D9DF7, 0x7F7DE8, 0xC77FEA, 0xF2677F, 0xF5B04A, 0x5CCB79, 0x7FCDEB, 0xF2F2F5,
    ];
    let reference_gradients = [
        (0xA978FF, 0x3FA2FF),
        (0xF95B7D, 0xFAB942),
        (0x46E5AF, 0x42ADF1),
        (0xF760A8, 0xFCBB97),
        (0xFFE94A, 0x3FD3FF),
        (0xEC447B, 0x8E54DC),
        (0xFAD750, 0xF79445),
        (0xF7F8FA, 0x9EABBA),
    ];
    assert_eq!(ACCENT_SOLIDS.map(|(_, color)| color), reference_solids);
    assert_eq!(
        ACCENT_GRADIENTS.map(|(_, a, b)| (a, b)),
        reference_gradients
    );
    let mut state = ready();
    state.open_settings();
    state.section = crate::state::APPEARANCE_SECTION;
    state.settings_level = crate::state::SettingsLevel::Section;
    let scene = scene(&state);
    let right = 832.;
    let top = 184.;
    let safe = 808.;
    assert!(scene.draw.iter().any(
        |d| matches!(d,crate::renderer::Draw::Clip(Some(r)) if r.x>=right && r.y>=top && r.h<=safe)
    ));
}
#[test]
fn nav_labels_do_not_use_heading_font_by_size() {
    let mut state = ready();
    state.open(0);
    let scene = scene(&state);
    for draw in &scene.draw {
        if let crate::renderer::Draw::Text(label, _, style, _, _) = draw
            && [
                "Home",
                "Apps",
                "Settings",
                "Next video",
                "Video gallery",
                "Netflix",
            ]
            .contains(&label.as_str())
        {
            assert_eq!(
                style.display,
                Some(false),
                "interactive label {label} is display"
            );
        }
    }
}

#[test]
fn upgraded_2240_preserves_original_approved_palette() {
    let mut state = ready();
    state.preferences.accent.kind = "gradient".into();
    state.preferences.accent.id = "Citrus".into();
    state.preferences.accent.normalized();
    assert_eq!(state.preferences.accent.colors(), (0xFFE94A, 0x3FD3FF));
    assert_eq!(crate::model::ACCENT_SOLIDS.len(), 8);
    assert_eq!(crate::model::ACCENT_GRADIENTS.len(), 8);
}

#[test]
fn ui_resolution_modes_are_bounded_and_persisted() {
    use crate::model::{Preferences, UiResolution};
    assert_eq!(UiResolution::ALL.len(), 4);
    assert_eq!(UiResolution::Auto.pixels(), crate::raster::Size::UHD);
    assert_eq!(
        UiResolution::Qhd.pixels(),
        crate::raster::Size::new(2560, 1440)
    );
    assert_eq!(UiResolution::Hd.pixels(), crate::raster::Size::HD);
    assert_eq!(UiResolution::Uhd.lower(), UiResolution::Qhd);
    assert_eq!(UiResolution::Qhd.lower(), UiResolution::Hd);
    assert_eq!(UiResolution::Hd.lower(), UiResolution::Hd);
    let mut prefs = Preferences::default();
    assert_eq!(prefs.ui_resolution, UiResolution::Qhd);
    prefs.ui_resolution = UiResolution::Auto;
    assert_eq!(prefs.ui_resolution_value(), "Auto · 4K");
    prefs.auto_ui_cap = UiResolution::Qhd;
    assert_eq!(prefs.ui_resolution_value(), "Auto · 1440p");
    prefs.ui_resolution = UiResolution::Hd;
    assert_eq!(prefs.resolved_ui_size(), crate::raster::Size::HD);
    let encoded = serde_json::to_string(&prefs).unwrap();
    let restored: Preferences = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored.ui_resolution, UiResolution::Hd);
    assert_eq!(restored.auto_ui_cap, UiResolution::Qhd);
    let legacy: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.resolved_ui_size(), UiResolution::Qhd.pixels());
}

#[test]
fn ui_resolution_selection_back_restores_appearance_and_remote_navigation() {
    use crate::model::UiResolution;
    use crate::state::{APPEARANCE_SECTION, SettingsLevel};
    let mut state = ready();
    legacy_look(&mut state.preferences);
    state.preferences.reduced_motion = true;
    state.open_settings();
    state.focus = Target::Sidebar(APPEARANCE_SECTION);
    state.activate();
    let appearance = scene(&state);
    assert!(
        appearance
            .controls
            .iter()
            .any(|c| c.target == Target::UiResolutionOpen)
    );
    state.focus = Target::UiResolutionOpen;
    state.activate();
    assert_eq!(state.settings_level, SettingsLevel::UiResolution);
    assert_eq!(state.focus, Target::UiResolutionPick(0));
    let choices = scene(&state);
    assert_eq!(
        choices
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::UiResolutionPick(_)))
            .count(),
        4
    );
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::UiResolutionPick(1));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::UiResolutionPick(2));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.ui_resolution, UiResolution::Qhd);
    state.back();
    assert_eq!(state.settings_level, SettingsLevel::Section);
    assert_eq!(state.focus, Target::UiResolutionOpen);
    state.back();
    assert_eq!(state.settings_level, SettingsLevel::Root);
    assert_eq!(state.focus, Target::Sidebar(APPEARANCE_SECTION));
}

#[test]
fn nunito_heading_and_body_are_independent_and_live() {
    use crate::typography::{HeadingFont, TextFont};
    let mut state = ready();
    state.preferences.heading_font = HeadingFont::Nunito;
    state.preferences.text_font = TextFont::Inter;
    assert_eq!(state.preferences.heading_font.name(), "Nunito");
    assert!(HeadingFont::ALL.contains(&HeadingFont::Nunito));
    assert!(TextFont::ALL.contains(&TextFont::Nunito));
    assert!(HeadingFont::Nunito.scale() >= 0.9 && HeadingFont::Nunito.scale() <= 1.14);
    let prefs = serde_json::to_string(&state.preferences).unwrap();
    assert!(prefs.contains("\"heading_font\":\"nunito\""));
    state.preferences.text_font = TextFont::Nunito;
    assert_eq!(state.preferences.text_font.id(), "nunito");
}

#[test]
fn large_weather_heading_wraps_at_most_twice_and_pushes_favorites_below_subtitle() {
    use crate::renderer::Draw;
    use crate::weather_phrases::{Category, Phrase};
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.preferences.heading_size_pct = 170;
    state.preferences.text_size_pct = 150;
    state.rude_weather.current = Some(Phrase {
        category: Category::Cloudy,
        title: "Another ridiculous grey weather headline with several words".into(),
        subtitle: "Nothing is cooperating today".into(),
    });
    let rendered = scene(&state);
    let mut heading_lines: Vec<f32> = Vec::new();
    let mut subtitle_bottom = 0.0f32;
    let mut favorites_y = None;
    for draw in rendered.draw {
        if let Draw::Text(ref label, rect, style, _, _) = draw {
            if label == "Another "
                || label == "ridiculous "
                || label == "grey "
                || label == "weather "
                || label == "headline "
                || label == "with "
                || label == "several "
                || label == "words"
            {
                if !heading_lines.iter().any(|&y| (y - rect.y).abs() < 0.5) {
                    heading_lines.push(rect.y);
                }
                assert!(style.display == Some(true));
            }
            if label == "today" {
                subtitle_bottom = rect.y + rect.h;
            }
            if label == "Favorite apps" {
                favorites_y = Some(rect.y);
            }
        }
    }
    assert!(!heading_lines.is_empty(), "Rude headline is visible");
    assert!(
        heading_lines.len() <= 2,
        "Rude headline must fit two lines: {heading_lines:?}"
    );
    assert!(subtitle_bottom > heading_lines.iter().copied().fold(0.0f32, f32::max));
    assert!(favorites_y.expect("Favorites") >= subtitle_bottom + 2.);
}
#[test]
fn heading_text_size_bounds_keep_new_upper_limits_after_prefs_roundtrip() {
    let mut prefs = Preferences {
        heading_size_pct: 255,
        text_size_pct: 255,
        ..Default::default()
    };
    prefs.sanitize();
    assert_eq!(prefs.heading_size_pct, 170);
    assert_eq!(prefs.text_size_pct, 150);
    let stored = serde_json::to_string(&prefs).unwrap();
    let loaded: Preferences = serde_json::from_str(&stored).unwrap();
    assert_eq!(loaded.heading_size_pct, 170);
    assert_eq!(loaded.text_size_pct, 150);
}

/// The pre-25.10 defaults that many layout tests were written against.
fn legacy_look(prefs: &mut Preferences) {
    prefs.accent = crate::model::Accent {
        kind: "solid".into(),
        id: "White".into(),
        use_focus: false,
        use_nav: false,
        use_headline: false,
    };
    prefs.heading_font = crate::typography::HeadingFont::Archivo;
    prefs.text_font = crate::typography::TextFont::Inter;
    prefs.heading_size_pct = 100;
    prefs.text_size_pct = 100;
    prefs.phrase_subtitle_pct = 100;
    prefs.ui_resolution = crate::model::UiResolution::Auto;
}

#[test]
fn leaving_a_scrolled_stack_row_glides_home_with_one_small_swing() {
    use crate::state::{STACK_RETURN_DELAY, STACK_RETURN_TIME, stack_return_curve};
    for offset in [300., 1200., 5000.] {
        assert_eq!(stack_return_curve(offset, STACK_RETURN_DELAY * 0.5), offset);
        let mid = stack_return_curve(offset, STACK_RETURN_DELAY + 0.15);
        assert!(mid > 0. && mid < offset);
        let lowest = (0..400)
            .map(|i| stack_return_curve(offset, i as f32 * STACK_RETURN_TIME / 400.))
            .fold(f32::MAX, f32::min);
        assert!(
            lowest < 0. && lowest > -41. && lowest > -offset * 0.05,
            "overshoot {lowest}"
        );
        let end = stack_return_curve(offset, STACK_RETURN_TIME - 0.01);
        assert!(end.abs() < 4., "settled {end}");
        const { assert!(STACK_RETURN_TIME <= 0.45) };
        // One swing only: once it starts coming back it never turns again.
        let samples: Vec<f32> = (0..=400)
            .map(|i| stack_return_curve(offset, i as f32 * STACK_RETURN_TIME / 400.))
            .collect();
        let turns = samples
            .windows(3)
            .filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.)
            .count();
        assert_eq!(turns, 1, "wobble");
        assert_eq!(stack_return_curve(offset, STACK_RETURN_TIME + 0.01), 0.);
    }
}

#[test]
fn stack_card_opens_title_card_and_back_returns_to_the_card() {
    use crate::media::{Detail, PlayRequest};
    use crate::stacks::{StackConfig, StackItem};
    let mut state = ready();
    state.preferences.stacks = vec![StackConfig {
        source: "jellyfin".into(),
        ..Default::default()
    }];
    state.stack_items = vec![
        (1..=3)
            .map(|id| StackItem {
                title: format!("Film {id}"),
                poster: None,
                id,
            })
            .collect(),
    ];
    state.focus = Target::StackCard(0, 1);
    assert_eq!(state.activate(), vec![Effect::MediaDetails(2)]);
    assert!(state.movie.is_some());
    assert_eq!(state.focus, Target::MoviePlay);
    state.movie_details(Detail {
        id: 2,
        title: "Film 2".into(),
        subtitles: vec![("English".into(), "http://x/en.srt".into())],
        qualities: vec!["1080p".into(), "720p".into()],
        ..Default::default()
    });
    // The card is drawn on top and takes every control.
    let targets: Vec<Target> = controls(&state).into_iter().map(|c| c.target).collect();
    assert!(targets.contains(&Target::MovieClose));
    assert!(!targets.iter().any(|t| matches!(t, Target::StackCard(..))));
    // The pickers sit in Play's row; OK opens the subtitles panel.
    state.direction(Direction::Right, &[]);
    assert_eq!(state.focus, Target::MoviePill(1));
    state.activate();
    assert_eq!(state.focus, Target::MovieOption(0));
    state.direction(Direction::Down, &[]);
    state.activate();
    assert_eq!(state.focus, Target::MoviePill(1));
    state.direction(Direction::Down, &[]);
    assert_eq!(
        state.activate(),
        vec![Effect::MediaPlay(PlayRequest {
            id: 2,
            episode: None,
            quality: None,
            subtitle: Some("English".into()),
        })]
    );
    // More like this lists the other cards of the row.
    state.direction(Direction::Down, &[]);
    assert_eq!(state.focus, Target::MovieSimilar(0));
    state.back();
    assert!(state.movie.is_none());
    assert_eq!(state.focus, Target::StackCard(0, 1));
}

#[test]
fn title_card_waits_for_details_then_comes_in_without_the_stand_in_title() {
    use crate::media::Detail;
    use crate::renderer::Draw;
    use crate::stacks::{StackConfig, StackItem};
    let mut state = ready();
    state.preferences.stacks = vec![StackConfig {
        source: "jellyfin".into(),
        ..Default::default()
    }];
    state.stack_items = vec![vec![StackItem {
        title: "Фильм / Film".into(),
        poster: None,
        id: 7,
    }]];
    state.focus = Target::StackCard(0, 0);
    state.activate();
    let visible = |state: &State, text: &str| {
        scene(state)
            .draw
            .iter()
            .any(|d| matches!(d, Draw::Text(t, _, _, color, _) if t == text && color[3] > 0.01))
    };
    // While the details travel, the stack title ("RU / EN") never shows.
    assert!(!visible(&state, "Фильм / Film"));
    state.movie_details(Detail {
        id: 7,
        title: "Фильм".into(),
        ..Default::default()
    });
    // The details start the entrance; it ends with everything in place.
    let m = state.movie.as_mut().unwrap();
    assert!(m.shown.is_some());
    m.opened = std::time::Instant::now() - std::time::Duration::from_secs(2);
    m.shown = Some(m.opened);
    assert!(visible(&state, "Фильм"));
    assert!(!visible(&state, "Фильм / Film"));
}

#[test]
fn root_status_reads_the_homebrew_exec_reply() {
    use crate::platform::parse_root_status;
    assert_eq!(parse_root_status("noroot\n"), None);
    assert_eq!(parse_root_status(""), None);
    let status = parse_root_status(
        "root\nwake\n{\n  \"settings\": { \"homeAutoLaunch\": \"off\" },\n  \"returnValue\": true\n}\n",
    )
    .unwrap();
    assert!(status.wake);
    assert_eq!(status.home_auto_launch, Some(false));
    let status = parse_root_status("root\n").unwrap();
    assert!(!status.wake);
    assert_eq!(status.home_auto_launch, None);
}

#[test]
fn auto_input_follows_the_hdmi_picture_not_only_the_cable() {
    let reply = json!({"returnValue": true,
        "HDMI1": {"5v": "TRUE", "signal": "TRUE"},
        "HDMI2": {"5v": "TRUE", "signal": "FALSE"},
        "HDMI3": {"5v": "FALSE", "signal": "FALSE"}});
    let signals = crate::platform::parse_signals(&reply);
    assert_eq!(signals, vec![(1, true), (2, false), (3, false)]);
    let mut inputs = demo_inventory().inputs;
    assert!(crate::model::apply_signals(&mut inputs, &signals));
    let preferences = Preferences::default();
    // Both cables are plugged in; only the device sending a picture shows.
    assert!(preferences.input_visible(&inputs[0]));
    assert!(!preferences.input_visible(&inputs[1]));
    assert!(!crate::model::apply_signals(&mut inputs, &signals));
}

fn ha_entity(id: &str, name: &str, state: &str) -> crate::smart_home::Entity {
    crate::smart_home::Entity {
        id: id.into(),
        name: name.into(),
        state: state.into(),
        domain: id.split_once('.').unwrap().0.into(),
        ..Default::default()
    }
}

#[test]
fn smart_home_recommends_controls_not_maintenance_entities() {
    let mut hidden = ha_entity("switch.porch", "Porch", "on");
    hidden.hidden = true;
    let mut door = ha_entity("binary_sensor.front", "Front door", "off");
    door.device_class = "door".into();
    let useful = [
        ha_entity("light.desk", "Desk", "on"),
        ha_entity("switch.soundbar", "Sony Soundbar", "off"),
        ha_entity("camera.yard", "Yard", "idle"),
        door,
    ];
    let noise = [
        hidden,
        ha_entity("switch.led_indicator", "Mini Switch Indicator", "on"),
        ha_entity("sensor.mini_battery", "Mini Switch Battery", "88"),
        ha_entity("binary_sensor.config_dirty", "Config Dirty", "off"),
        ha_entity("light.garden", "Garden", "unavailable"),
        ha_entity("sensor.power", "Power", "12"),
    ];
    assert!(useful.iter().all(|e| e.useful()));
    assert!(noise.iter().all(|e| !e.useful()));
}

#[test]
fn smart_home_shows_one_stream_per_camera() {
    use crate::smart_home::stream_twin;
    assert_eq!(
        stream_twin("camera.garden_sd_stream").as_deref(),
        Some("camera.garden_hd_stream")
    );
    assert_eq!(stream_twin("camera.garden_hd_stream"), None);
    assert!(ha_entity("camera.garden_hd_stream", "Garden HD", "idle").useful());
    assert!(!ha_entity("camera.garden_sd_stream", "Garden SD", "idle").useful());
    let mut state = crate::state::State::new(crate::model::Preferences::default());
    state.ha_entities = vec![
        ha_entity(
            "camera.outdoor_camera_hd_stream",
            "Outdoor Camera HD Stream",
            "idle",
        ),
        ha_entity(
            "camera.outdoor_camera_sd_stream",
            "Outdoor Camera SD Stream",
            "idle",
        ),
        ha_entity(
            "camera.living_room_cam_hd_stream",
            "Living Room Cam HD Stream",
            "idle",
        ),
        ha_entity(
            "camera.living_room_cam_sd_stream",
            "Living Room Cam SD Stream",
            "idle",
        ),
    ];
    let shown: Vec<_> = state.ha_tab(1).iter().map(|e| e.id.clone()).collect();
    assert_eq!(
        shown,
        [
            "camera.living_room_cam_hd_stream",
            "camera.outdoor_camera_hd_stream"
        ]
    );
}

#[test]
fn smart_home_tab_defaults_to_recommended_and_bulk_choices() {
    use crate::state::Target;
    let mut state = crate::state::State::new(crate::model::Preferences::default());
    let mut diagnostic = ha_entity("switch.reboot", "Reboot", "off");
    diagnostic.hidden = true;
    state.ha_entities = vec![
        ha_entity("light.desk", "Desk", "on"),
        ha_entity("sensor.mini_battery", "Mini Switch Battery", "88"),
        diagnostic,
    ];
    let ids = |state: &crate::state::State| -> Vec<String> {
        state.ha_tab(0).iter().map(|e| e.id.clone()).collect()
    };
    // Hidden/diagnostic entities are not even listed; noise is listed but off.
    assert_eq!(state.ha_selection_entries(0).len(), 2);
    assert_eq!(ids(&state), vec!["light.desk"]);
    state.focus = Target::SmartBulk(0, 1);
    state.activate();
    assert_eq!(ids(&state).len(), 2);
    state.focus = Target::SmartBulk(0, 2);
    state.activate();
    assert!(ids(&state).is_empty());
    state.focus = Target::SmartBulk(0, 0);
    state.activate();
    assert_eq!(ids(&state), vec!["light.desk"]);
}

#[test]
fn smart_home_old_everything_selection_is_reset_once() {
    let mut preferences = crate::model::Preferences::default();
    preferences.smart_home.selected_entities = vec!["sensor.noise".into()];
    let state = crate::state::State::new(preferences);
    assert!(state.preferences.smart_home.selected_entities.is_empty());
    assert_eq!(state.preferences.smart_home.selection_version, 1);
    let kept = crate::state::State::new(state.preferences.clone());
    assert_eq!(kept.preferences.smart_home.selection_version, 1);
}

#[test]
fn hidden_camera_snapshot_is_blurred_small() {
    let sharp = image::RgbImage::from_fn(640, 360, |x, y| {
        if (x / 8 + y / 8) % 2 == 0 {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([0, 0, 0])
        }
    });
    let mut jpeg = Vec::new();
    sharp
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let soft = crate::smart_home::blur_snapshot(&jpeg).unwrap();
    let soft = image::load_from_memory(&soft).unwrap().to_luma8();
    assert_eq!((soft.width(), soft.height()), (480, 270));
    // The checkerboard is gone: every pixel sits near the mid grey.
    assert!(soft.pixels().all(|p| (80..=176).contains(&p.0[0])));
}

#[test]
fn unsupported_saved_media_source_is_ignored_without_losing_other_settings() {
    let saved = serde_json::json!({
        "stacks": [
            {"source": "obsolete", "collection": "popular", "name": "Old row"},
            {"source": "jellyfin", "collection": "recent", "name": "Recent media"}
        ],
        "former_account": {"unexpected": "value"},
        "luma_player": true
    });
    let mut prefs: crate::model::Preferences = serde_json::from_value(saved).unwrap();
    prefs.sanitize();
    assert_eq!(prefs.stacks.len(), 1);
    assert_eq!(prefs.stacks[0].source, "jellyfin");
    assert_eq!(prefs.stacks[0].collection, "recent");
    assert!(prefs.luma_player);
}
