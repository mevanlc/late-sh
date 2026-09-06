use serde_json::{Value, json};

use super::statusline::{
    LabelMode, STATUS_COMPONENT_COUNT, StatusComponent, StatusComponentSetting, StatusVariant,
    default_statusline_components, normalize_statusline_components, parse_statusline_components,
    statusline_components_json,
};

fn find(
    components: &[StatusComponentSetting],
    component: StatusComponent,
) -> &StatusComponentSetting {
    components
        .iter()
        .find(|s| s.component == component)
        .expect("component present")
}

#[test]
fn default_list_covers_every_component_exactly_once() {
    let defaults = default_statusline_components();
    assert_eq!(defaults.len(), STATUS_COMPONENT_COUNT);
    for component in StatusComponent::ALL {
        assert_eq!(
            defaults.iter().filter(|s| s.component == component).count(),
            1,
            "{} appears once",
            component.as_str()
        );
    }
}

/// A user without a stored list sees the current upstream HUD content in its
/// current order, plus nothing opt-in.
#[test]
fn default_enabled_set_reproduces_the_upstream_bar() {
    let enabled: Vec<StatusComponent> = default_statusline_components()
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.component)
        .collect();
    assert_eq!(
        enabled,
        vec![
            StatusComponent::Pomodoro,
            StatusComponent::Voice,
            StatusComponent::Mentions,
            StatusComponent::Pot,
            StatusComponent::Chips,
        ]
    );
}

/// The pot is visible by default but remains the first ambient segment to yield;
/// everything opt-in also starts in that low-priority tier.
#[test]
fn ambient_and_opt_in_components_start_low_priority() {
    for setting in default_statusline_components() {
        assert_eq!(
            setting.low_priority,
            setting.component == StatusComponent::Pot || !setting.enabled,
            "{} priority tier",
            setting.component.as_str()
        );
    }
}

/// Only `Time` supplies its icon at render time (it is hour-dependent), so
/// every other component must carry one. The two-cells-wide invariant those
/// icons have to satisfy is locked in `late-ssh`, against the same measuring
/// function ratatui lays cells out with.
#[test]
fn every_component_but_time_carries_an_icon() {
    for component in StatusComponent::ALL {
        assert_eq!(
            component.icon().is_empty(),
            component == StatusComponent::Time,
            "{} icon",
            component.as_str()
        );
    }
}

#[test]
fn normalize_drops_duplicates_and_keeps_stored_order() {
    let stored = vec![
        StatusComponentSetting {
            enabled: true,
            ..StatusComponentSetting::new(StatusComponent::Chips)
        },
        StatusComponentSetting {
            enabled: false,
            ..StatusComponentSetting::new(StatusComponent::Mentions)
        },
        // Duplicate with a different reading: the first entry wins.
        StatusComponentSetting {
            enabled: false,
            ..StatusComponentSetting::new(StatusComponent::Chips)
        },
    ];

    let normalized = normalize_statusline_components(&stored);
    assert_eq!(normalized.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(normalized[0].component, StatusComponent::Chips);
    assert!(normalized[0].enabled);
    assert_eq!(normalized[1].component, StatusComponent::Mentions);
    assert!(!normalized[1].enabled);
}

/// Backfill diverges from the sidebar's blanket enable. The upstream pot is the
/// one required addition; optional components stay off in an existing custom
/// roster.
#[test]
fn normalize_backfills_each_component_at_its_own_policy() {
    let stored = vec![StatusComponentSetting::new(StatusComponent::Chips)];
    let normalized = normalize_statusline_components(&stored);

    assert_eq!(normalized.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(normalized[0].component, StatusComponent::Chips);
    for setting in normalized.iter().skip(1) {
        assert_eq!(
            setting.enabled,
            setting.component.backfill_existing(),
            "{} backfilled at its own policy",
            setting.component.as_str()
        );
        assert_eq!(
            setting.enabled,
            setting.component == StatusComponent::Pot,
            "only the upstream pot forces itself on"
        );
    }
}

#[test]
fn normalize_repairs_a_variant_that_does_not_belong_to_the_component() {
    let stored = vec![StatusComponentSetting {
        // A clock variant stored against the quests component: stale data, not
        // a reason to drop the component.
        variant: Some(StatusVariant::Clock24),
        ..StatusComponentSetting::new(StatusComponent::Quests)
    }];

    let normalized = normalize_statusline_components(&stored);
    assert_eq!(
        find(&normalized, StatusComponent::Quests).variant,
        Some(StatusVariant::QuestsDaily)
    );
}

#[test]
fn normalize_clears_auto_hide_on_components_that_cannot_hide() {
    let stored = vec![StatusComponentSetting {
        auto_hide: true,
        ..StatusComponentSetting::new(StatusComponent::Users)
    }];

    let normalized = normalize_statusline_components(&stored);
    assert!(!StatusComponent::Users.can_auto_hide());
    assert!(!find(&normalized, StatusComponent::Users).auto_hide);
}

#[test]
fn parse_skips_unknown_keys_and_falls_back_per_field() {
    let values = vec![
        json!({"key": "not_a_component", "enabled": true}),
        // Every dial absent: each one falls back to that component's default
        // rather than to a blanket value.
        json!({"key": "time"}),
        json!({"key": "chips", "enabled": false, "label": "bogus"}),
    ];

    let parsed = parse_statusline_components(&values);
    assert_eq!(parsed.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(parsed[0].component, StatusComponent::Time);

    let time = find(&parsed, StatusComponent::Time);
    assert_eq!(time.enabled, StatusComponent::Time.default_enabled());
    assert_eq!(time.label, LabelMode::None);
    assert_eq!(time.variant, Some(StatusVariant::Clock24));

    let chips = find(&parsed, StatusComponent::Chips);
    assert!(!chips.enabled);
    assert_eq!(chips.label, LabelMode::Text, "unreadable label mode");
    assert_eq!(chips.variant, None, "chips has no dial");
}

#[test]
fn json_round_trips_through_parse() {
    let mut components = default_statusline_components();
    components.swap(0, 3);
    components[0].enabled = true;
    components[0].label = LabelMode::Icon;
    components[0].low_priority = true;
    let time = components
        .iter_mut()
        .find(|s| s.component == StatusComponent::Time)
        .expect("time present");
    time.variant = Some(StatusVariant::ClockAmPm);

    let json = statusline_components_json(&components);
    let values = json.as_array().expect("array").clone();
    assert_eq!(parse_statusline_components(&values), components);
}

#[test]
fn parse_of_an_empty_array_yields_the_full_default_list() {
    // An empty stored array is a customized bar with everything removed, which
    // normalize backfills rather than leaving the user with no roster at all.
    let parsed = parse_statusline_components(&[] as &[Value]);
    assert_eq!(parsed.len(), STATUS_COMPONENT_COUNT);
    assert!(find(&parsed, StatusComponent::Pot).enabled);
    assert!(
        parsed
            .iter()
            .filter(|setting| setting.component != StatusComponent::Pot)
            .all(|setting| !setting.enabled),
        "optional backfilled entries stay off"
    );
}

#[test]
fn label_mode_cycles_both_ways_and_wraps() {
    assert_eq!(LabelMode::Text.cycle(true), LabelMode::Icon);
    assert_eq!(LabelMode::Icon.cycle(true), LabelMode::None);
    assert_eq!(LabelMode::None.cycle(true), LabelMode::Text);
    assert_eq!(LabelMode::Text.cycle(false), LabelMode::None);
}

#[test]
fn component_keys_and_variant_keys_round_trip() {
    for component in StatusComponent::ALL {
        assert_eq!(
            StatusComponent::from_key(component.as_str()),
            Some(component)
        );
        for variant in component.variants() {
            assert_eq!(StatusVariant::from_key(variant.as_str()), Some(*variant));
        }
    }
}

/// A component with a dial must also name it, or the customizer's detail pane
/// has a control with no heading.
#[test]
fn every_component_with_variants_has_a_variant_title() {
    for component in StatusComponent::ALL {
        assert_eq!(
            component.variants().is_empty(),
            component.variant_title().is_none(),
            "{} dial and heading disagree",
            component.as_str()
        );
    }
}
