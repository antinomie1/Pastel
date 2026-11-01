use pastel_config::keys::{ALT, CTRL, LOGO, SHIFT, parse_modifier};
use pastel_config::{Action, Combo, Config, Rgba, Section};
use pastel_layout::{CenterFocus, Direction, LayoutOp, WorkspaceTarget};

fn combo(text: &str) -> Combo {
    text.parse().expect("combo parses")
}

fn action(text: &str) -> Action {
    text.parse().expect("action parses")
}

#[test]
fn an_empty_config_is_a_working_desktop() {
    let config = Config::parse("").expect("defaults are valid");
    assert_eq!(config.layout.kind, "scrolling");
    let table = config.binds.table(config.input.mod_key);
    let quit = Combo {
        mods: LOGO | SHIFT,
        key: "e".into(),
    };
    assert_eq!(table.get(&quit), Some(&Action::Quit));
    assert!(table.len() > 50);
}

#[test]
fn the_example_config_is_valid_and_says_what_the_defaults_say() {
    let text = include_str!("../../../config.example.toml");
    let config = Config::parse(text).expect("example parses");
    let defaults = Config::parse("").expect("defaults are valid");
    assert_eq!(config.layout, defaults.layout);
    assert_eq!(config.window, defaults.window);
    assert_eq!(config.input, defaults.input);
    assert_eq!(config.overlay, defaults.overlay);
}

#[test]
fn every_default_action_with_a_title_has_a_section() {
    let config = Config::parse("").expect("defaults are valid");
    let titled = config.binds.iter().filter_map(|(_, a)| a.title());
    let sections: Vec<Section> = titled.map(|(s, _)| s).collect();
    for section in Section::ALL {
        assert!(
            sections.contains(&section),
            "nothing under {}",
            section.title()
        );
    }
}

#[test]
fn unknown_keys_are_errors_not_silence() {
    assert!(Config::parse("[layout]\ngaps = 3").is_err());
    assert!(Config::parse("nonsense = 1").is_err());
    assert!(Config::parse("[window]\nblurr = true").is_err());
}

#[test]
fn bad_values_say_what_is_wrong() {
    let error = |text: &str| Config::parse(text).err().expect("should fail");
    assert!(error("[layout]\ndefault-width = 2.0").contains("default-width"));
    assert!(error("[layout]\npreset-widths = [0.5, 0.0]").contains("preset-widths"));
    assert!(error("[layout]\ntype = \"spiral\"").contains("scrolling"));
    assert!(error("[layout]\ncenter-focus = \"sometimes\"").contains("center-focus"));
    assert!(error("[layout]\ngap = -1.0").contains("gap"));
    assert!(error("[window]\nactive-color = \"red\"").contains("colour"));
    assert!(error("[input]\nmod = \"hyper\"").contains("hyper"));
    assert!(error("[animation]\nslowdown = 0.0").contains("slowdown"));
    assert!(error("[binds]\n\"Mod+Q\" = \"explode\"").contains("explode"));
    assert!(error("[binds]\n\"Hyper+Q\" = \"quit\"").contains("hyper"));
}

#[test]
fn user_binds_merge_over_the_defaults() {
    let config = Config::parse(
        r#"
        [binds]
        "Mod+Q" = "quit"
        "Mod+T" = "spawn alacritty -e htop"
        "Mod+D" = "none"
        "#,
    )
    .expect("parses");
    let table = config.binds.table(LOGO);
    let key = |k: &str| Combo {
        mods: LOGO,
        key: k.into(),
    };
    assert_eq!(table.get(&key("q")), Some(&Action::Quit));
    assert_eq!(table.get(&key("d")), None);
    let spawn = Action::Spawn(vec!["alacritty".into(), "-e".into(), "htop".into()]);
    assert_eq!(table.get(&key("t")), Some(&spawn));
    assert!(table.contains_key(&key("h")), "other defaults survive");
}

#[test]
fn nested_mod_is_separate_from_the_real_one() {
    let config = Config::parse("").expect("defaults are valid");
    assert_eq!(config.input.mod_key, LOGO);
    assert_eq!(config.input.nested_mod_key, ALT);
    assert!(config.binds.table(ALT).contains_key(&Combo {
        mods: ALT,
        key: "q".into()
    }));
}

#[test]
fn layout_section_reaches_the_options() {
    let config = Config::parse(
        "[layout]\ngap = 4\ncenter-focus = \"on-overflow\"\npreset-widths = [0.75, 0.25]",
    )
    .expect("parses");
    assert_eq!(config.layout.gap, 4.0);
    assert_eq!(config.layout.center_focus, CenterFocus::OnOverflow);
    assert_eq!(
        config.layout.preset_widths,
        [0.25, 0.75],
        "sorted for cycling"
    );
}

#[test]
fn animations_can_be_tuned_or_turned_off() {
    let tuned = Config::parse(
        r#"
        [animation]
        slowdown = 2.0
        window-movement = { stiffness = 300.0, damping = 0.8 }
        window-open = { ms = 500.0, easing = "out-expo" }
        "#,
    )
    .expect("parses");
    assert!(
        !tuned.animation.window_open.sample(0.9, 0.0).done,
        "slowdown stretches"
    );
    assert!(tuned.animation.window_open.sample(1.0, 0.0).done);
    let off = Config::parse("[animation]\nenabled = false").expect("parses");
    assert!(off.animation.window_movement.sample(0.0, 0.0).done);
    assert!(Config::parse("[animation]\nwindow-open = { ms = 5.0 }").is_err());
    assert!(Config::parse("[animation]\nwindow-open = { ms = 5.0, easing = \"wobbly\" }").is_err());
}

#[test]
fn rules_match_by_substring_and_later_ones_win() {
    let config = Config::parse(
        r#"
        [[window-rule]]
        opacity = 0.95

        [[window-rule]]
        app-id = "foot"
        opacity = 0.8
        default-width = 0.33

        [[window-rule]]
        title = "Secret"
        app-id = "firefox"
        blur = false
        "#,
    )
    .expect("parses");
    let foot = config.rule_for("Foot", "shell");
    assert_eq!(
        foot.opacity,
        Some(0.95),
        "case-sensitive: only the catch-all matched"
    );
    assert_eq!(
        config.rule_for("org.foot.Foot", "shell").opacity,
        Some(0.8),
        "substring"
    );
    let foot = config.rule_for("foot", "shell");
    assert_eq!(foot.opacity, Some(0.8));
    assert_eq!(foot.default_width, Some(0.33));
    assert_eq!(config.rule_for("kitty", "x").opacity, Some(0.95));
    assert_eq!(
        config.rule_for("firefox", "x").blur,
        None,
        "needs both patterns"
    );
    assert_eq!(config.rule_for("firefox", "Secret tab").blur, Some(false));
}

#[test]
fn combos_parse_modifiers_in_any_case_and_order() {
    assert_eq!(
        combo("Ctrl+Alt+Delete"),
        Combo {
            mods: CTRL | ALT,
            key: "Delete".into()
        }
    );
    assert_eq!(
        combo("shift+control+x"),
        Combo {
            mods: CTRL | SHIFT,
            key: "x".into()
        }
    );
    assert_eq!(combo("Super+Q"), combo("logo+q"));
    assert_eq!(
        combo("F11"),
        Combo {
            mods: 0,
            key: "F11".into()
        }
    );
    assert!("Mod+".parse::<Combo>().is_err());
    assert!("".parse::<Combo>().is_err());
    assert!("Meta+Q".parse::<Combo>().is_err());
}

#[test]
fn only_mod_resolves() {
    assert_eq!(
        combo("Mod+Shift+H").resolved(ALT),
        Combo {
            mods: ALT | SHIFT,
            key: "h".into()
        }
    );
    assert_eq!(
        combo("Ctrl+H").resolved(ALT),
        Combo {
            mods: CTRL,
            key: "h".into()
        }
    );
}

#[test]
fn labels_read_like_keys_not_keysyms() {
    assert_eq!(combo("Mod+Shift+h").label(LOGO), "Mod + Shift + H");
    assert_eq!(combo("Mod+Return").label(LOGO), "Mod + Return");
    assert_eq!(combo("Mod+bracketleft").label(LOGO), "Mod + [");
    assert_eq!(combo("Ctrl+Alt+Delete").label(LOGO), "Ctrl + Alt + DELETE");
}

#[test]
fn rebinding_a_combo_replaces_it() {
    let mut binds = pastel_config::Binds::default();
    binds.set(combo("Mod+A"), Action::Quit);
    binds.set(combo("Mod+A"), Action::CloseWindow);
    assert_eq!(binds.iter().count(), 1);
    binds.set(combo("Mod+A"), Action::None);
    assert_eq!(binds.iter().count(), 0);
}

#[test]
fn modifier_names() {
    assert_eq!(parse_modifier("Alt"), Ok(ALT));
    assert_eq!(parse_modifier("win"), Ok(LOGO));
    assert!(parse_modifier("meta").is_err());
}

#[test]
fn actions_parse_with_and_without_arguments() {
    assert_eq!(action("quit"), Action::Quit);
    assert_eq!(
        action("  focus-column-left "),
        Action::Layout(LayoutOp::Focus(Direction::Left))
    );
    assert_eq!(
        action("focus-workspace 3"),
        Action::FocusWorkspace(WorkspaceTarget::Index(2))
    );
    assert_eq!(
        action("move-to-workspace 1"),
        Action::MoveToWorkspace(WorkspaceTarget::Index(0))
    );
    assert_eq!(action("spawn foot"), Action::Spawn(vec!["foot".into()]));
    assert!("spawn".parse::<Action>().is_err());
    assert!("quit now".parse::<Action>().is_err());
    assert!("focus-workspace 0".parse::<Action>().is_err());
    assert!("focus-workspace two".parse::<Action>().is_err());
}

#[test]
fn every_listed_name_parses() {
    for name in Action::names() {
        let filled = name.replace("<command>", "foot").replace("<n>", "1");
        assert!(filled.parse::<Action>().is_ok(), "{name}");
    }
}

#[test]
fn overlay_titles_are_curated() {
    assert_eq!(Action::None.title(), None);
    let spawn = Action::Spawn(vec!["/usr/bin/foot".into()]);
    assert_eq!(spawn.title().expect("titled").1, "Run foot");
    assert!(Action::CloseWindow.title().is_some());
}

#[test]
fn overlay_rows_fold_keys_that_share_a_title() {
    let mut binds = pastel_config::Binds::default();
    binds.set(combo("Mod+Q"), Action::Quit);
    binds.set(combo("Mod+H"), action("focus-column-left"));
    binds.set(combo("Mod+L"), action("focus-column-right"));
    binds.set(combo("Ctrl+Left"), action("focus-column-left"));
    let rows = binds.overlay_rows(LOGO);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].section,
        Section::Windows,
        "section order, not bind order"
    );
    assert_eq!(rows[0].title, "Focus column");
    assert_eq!(
        rows[0].keys,
        "Mod + H / Mod + L / Ctrl + LEFT".replace("LEFT", "Left")
    );
    assert_eq!(rows[1].keys, "Mod + Q");
}

#[test]
fn overlay_rows_shorten_a_shared_prefix_and_truncate() {
    let config = Config::parse("").expect("defaults are valid");
    let rows = config.binds.overlay_rows(LOGO);
    let row = rows
        .iter()
        .find(|r| r.title == "Go to workspace")
        .expect("row");
    assert_eq!(row.keys, "Mod + 1 / 2 / 3 / 4 …");
    let row = rows
        .iter()
        .find(|r| r.title == "Focus column")
        .expect("row");
    assert_eq!(row.keys, "Mod + H / L / Left / Right");
    assert!(rows.iter().any(|r| r.title == "Run foot"));
}

#[test]
fn colours_parse_in_every_form() {
    assert_eq!(Rgba::parse("#ffffff"), Ok(Rgba::new(1.0, 1.0, 1.0, 1.0)));
    assert_eq!(Rgba::parse("#000"), Ok(Rgba::new(0.0, 0.0, 0.0, 1.0)));
    assert_eq!(Rgba::parse("#ff000000"), Ok(Rgba::new(1.0, 0.0, 0.0, 0.0)));
    let short = Rgba::parse("#f008").expect("parses");
    assert!((short.a - 8.0 / 15.0).abs() < 1e-6);
    for bad in ["ffffff", "#ff", "#gggggg", "#ffffffffff", "#ééé"] {
        assert!(Rgba::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn colours_interpolate() {
    let (a, b) = (Rgba::new(0.0, 0.0, 0.0, 1.0), Rgba::new(1.0, 0.5, 0.0, 0.0));
    assert_eq!(a.lerp(b, 0.5), Rgba::new(0.5, 0.25, 0.0, 0.5));
}
