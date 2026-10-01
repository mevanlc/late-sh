use std::time::Duration;

use late_core::models::{profile::Profile, rss_feed::RssFeed, user::InteractionMode};
use late_core::test_utils::{TestDb, create_test_user};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::mouse::{Field, Target};
use super::state::{AccountRow, Row, StatuslinePane, Tab, TweakRow};
use crate::app::common::sidebar::SidebarOwnership;
use crate::app::state::App;
use crate::test_helpers::{make_app, new_test_db};

async fn fixture() -> (TestDb, App) {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "mouse-user").await;
    let mut app = make_app(db.db.clone(), user.id, "settings-mouse-test");
    app.interaction_mode = InteractionMode::Hybrid;
    let client = db.db.get().await.unwrap();
    let profile = Profile::load(&client, user.id).await.unwrap();
    app.settings_modal_state
        .open_from_profile(&profile, app.rail_modes());
    app.show_settings = true;
    (db, app)
}

fn paint(app: &App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(app.size.0, app.size.1)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            super::ui::draw(
                frame,
                area,
                &app.settings_modal_state,
                SidebarOwnership {
                    pet: true,
                    tank: true,
                },
            );
            if app.tag_picker.is_open() {
                crate::app::tag_picker::ui::draw(frame, area, &app.tag_picker);
            }
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn hits(app: &App) -> Vec<(ratatui::layout::Rect, Target)> {
    if app.tag_picker.is_open() {
        app.tag_picker.mouse.hits.borrow().clone()
    } else {
        app.settings_modal_state.mouse.hits.borrow().clone()
    }
}

fn click(app: &mut App, target: Target) {
    paint(app);
    let rect = hits(app)
        .iter()
        .rev()
        .find_map(|(rect, hit)| (*hit == target).then_some(*rect))
        .unwrap_or_else(|| panic!("no visible target {target:?}: {:?}", hits(app)));
    app.handle_input(format!("\x1b[<0;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
}

fn wheel(app: &mut App, target: Target, down: bool) {
    paint(app);
    let rect = hits(app)
        .iter()
        .find_map(|(rect, hit)| (*hit == target).then_some(*rect))
        .unwrap();
    app.handle_input(
        format!(
            "\x1b[<{};{};{}M",
            if down { 65 } else { 64 },
            rect.x + 1,
            rect.y + 1
        )
        .as_bytes(),
    );
}

async fn settle(app: &mut App, condition: impl Fn(&App) -> bool) {
    for _ in 0..1500 {
        app.tick();
        if condition(app) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition did not settle");
}

fn text(buffer: &Buffer) -> String {
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[tokio::test]
async fn settings_mouse_rendering_covers_every_tab_and_short_scrolled_rows() {
    let (_db, mut app) = fixture().await;
    for size in [(120, 40), (48, 14)] {
        app.resize(size.0, size.1).unwrap();
        for tab in Tab::ALL {
            app.settings_modal_state.select_tab(tab);
            let buffer = paint(&app);
            let all = hits(&app);
            for (rect, _) in &all {
                assert!(!rect.is_empty());
                assert!(rect.right() <= size.0 && rect.bottom() <= size.1);
            }
            for tab in Tab::ALL {
                assert!(all.iter().any(|(_, target)| *target == Target::Tab(tab)));
            }
            assert!(all.iter().any(|(_, target)| *target == Target::Close));
            assert!(!text(&buffer).is_empty());
        }
        app.settings_modal_state.select_tab(Tab::Settings);
        let mut seen = Vec::new();
        for _ in 0..12 {
            paint(&app);
            let all = hits(&app);
            seen.extend(all.iter().filter_map(|(_, target)| {
                if let Target::Row(row) = target {
                    Some(*row)
                } else {
                    None
                }
            }));
            let rect = all
                .iter()
                .find_map(|(rect, target)| matches!(target, Target::Row(_)).then_some(*rect))
                .unwrap();
            app.handle_input(format!("\x1b[<65;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
        }
        for row in Row::ALL {
            assert!(seen.contains(&row), "{size:?}: missing {row:?}");
        }
    }
}

#[tokio::test]
async fn settings_mouse_picker_filter_languages_and_foreground_priority() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Row(Row::Country));
    let old = app.settings_modal_state.draft().country.clone();
    wheel(&mut app, Target::Pick(0), true);
    assert_eq!(app.settings_modal_state.picker().selected_index, 0);
    assert_eq!(app.settings_modal_state.draft().country, old);
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, target)| matches!(target, Target::Tab(_)))
    );
    let index = hits(&app)
        .iter()
        .find_map(|(_, target)| {
            if let Target::Pick(index) = target {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    click(&mut app, Target::Pick(index));
    assert!(!app.settings_modal_state.picker_open());
    assert!(app.settings_modal_state.draft().country.is_some());
    click(&mut app, Target::Row(Row::Timezone));
    app.handle_input(b"nonsense-zone");
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, target)| matches!(target, Target::Pick(_)))
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Row(Row::Langs));
    let index = hits_after_paint(&app)
        .iter()
        .find_map(|(_, target)| {
            if let Target::Pick(index) = target {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    click(&mut app, Target::Pick(index));
    assert_eq!(app.tag_picker.chosen().len(), 1);
    let cursor = app.tag_picker.cursor();
    wheel(&mut app, Target::Pick(index), true);
    assert_eq!(app.tag_picker.cursor(), cursor);
    assert_eq!(app.tag_picker.chosen().len(), 1);
    click(&mut app, Target::Close);
    assert!(!app.tag_picker.is_open());
    assert_eq!(app.settings_modal_state.draft().langs.len(), 1);
}

fn hits_after_paint(app: &App) -> Vec<(ratatui::layout::Rect, Target)> {
    paint(app);
    hits(app)
}

#[tokio::test]
async fn settings_mouse_toggles_reorders_and_scrolls_status_panes_independently() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Tweaks));
    let original = app.settings_modal_state.draft().enable_background_color;
    click(&mut app, Target::Tweak(TweakRow::BackgroundColor));
    assert_eq!(
        app.settings_modal_state.draft().enable_background_color,
        !original
    );
    let rails = app.rail_modes();
    let default_sidebar = app.settings_modal_state.draft().right_sidebar_mode;
    click(&mut app, Target::SidebarMode);
    assert_ne!(app.rail_modes().1, rails.1);
    assert_eq!(app.settings_modal_state.device_rails(), app.rail_modes());
    assert_eq!(
        app.settings_modal_state.draft().right_sidebar_mode,
        default_sidebar
    );
    click(&mut app, Target::Tweak(TweakRow::RoomListSidebar));
    assert_ne!(app.rail_modes().0, rails.0);
    click(&mut app, Target::Tweak(TweakRow::RightSidebar));
    let components = app.settings_modal_state.right_sidebar_components().to_vec();
    click(&mut app, Target::SidebarMove(0, 1));
    assert_eq!(
        app.settings_modal_state.right_sidebar_components()[1],
        components[0]
    );
    click(&mut app, Target::Sidebar(1));
    assert_ne!(
        app.settings_modal_state.right_sidebar_components()[1].enabled,
        components[0].enabled
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Tweak(TweakRow::ChatBadges));
    click(&mut app, Target::Badge(0));
    click(&mut app, Target::Close);
    click(&mut app, Target::Tab(Tab::Statusline));
    let components = app.settings_modal_state.statusline_components().to_vec();
    click(&mut app, Target::StatusMove(0, 1));
    assert_eq!(
        app.settings_modal_state.statusline_components()[1],
        components[0]
    );
    click(&mut app, Target::StatusToggle(1));
    assert_ne!(
        app.settings_modal_state.statusline_components()[1].enabled,
        components[0].enabled
    );
    click(&mut app, Target::Status(1));
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::Detail
    );
    let before = app.settings_modal_state.statusline_components().to_vec();
    click(&mut app, Target::Dial(0));
    assert_ne!(app.settings_modal_state.statusline_components(), before);
    app.resize(48, 10).unwrap();
    paint(&app);
    assert!(hits(&app).iter().any(|(_, hit)| *hit == Target::Dial(0)));
    app.resize(60, 14).unwrap();
    let before = app.settings_modal_state.statusline_components().to_vec();
    wheel(&mut app, Target::Status(1), true);
    assert_eq!(app.settings_modal_state.statusline_components(), before);
    assert_eq!(app.settings_modal_state.statusline_index(), 1);
    let list_hits = hits_after_paint(&app)
        .into_iter()
        .filter(|(_, hit)| matches!(hit, Target::Status(_)))
        .collect::<Vec<_>>();
    if hits(&app).iter().any(|(_, hit)| *hit == Target::Dial(0)) {
        wheel(&mut app, Target::Dial(0), true);
    }
    let after = hits_after_paint(&app)
        .into_iter()
        .filter(|(_, hit)| matches!(hit, Target::Status(_)))
        .collect::<Vec<_>>();
    assert_eq!(list_hits, after);
    assert_eq!(app.settings_modal_state.statusline_components(), before);
}

#[tokio::test]
async fn settings_mouse_saves_before_navigation_and_retains_rejected_username() {
    let (db, mut app) = fixture().await;
    let _taken = create_test_user(&db.db, "taken-name").await;
    click(&mut app, Target::Row(Row::Username));
    app.handle_input(b"\x15taken-name");
    click(&mut app, Target::Tab(Tab::Account));
    assert!(app.settings_modal_state.mouse_save_pending());
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    // Duplicate requests cannot enqueue another destination or erase text.
    super::input::activate_mouse_target(&mut app, Target::Tab(Tab::Bio));
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    assert!(app.settings_modal_state.editing_username());
    assert_eq!(
        app.settings_modal_state.username_input().lines(),
        &["taken-name"]
    );
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    assert!(
        app.settings_modal_state
            .mouse_error
            .as_deref()
            .unwrap()
            .contains("taken")
    );
    click(&mut app, Target::Cancel);
    assert!(!app.settings_modal_state.editing_username());
    assert!(app.settings_modal_state.mouse_error.is_none());
    click(&mut app, Target::Row(Row::Username));
    app.handle_input(b"\x15fresh-name");
    click(&mut app, Target::Row(Row::Ide));
    assert!(app.settings_modal_state.editing_username());
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    assert!(!app.settings_modal_state.editing_username());
    assert!(app.settings_modal_state.editing_system_field().is_some());
    app.handle_input(b"my editor");
    click(&mut app, Target::Tab(Tab::Bio));
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Bio);
    let client = db.db.get().await.unwrap();
    let stored = Profile::load(&client, app.user_id).await.unwrap();
    assert_eq!(stored.username, "fresh-name");
    assert_eq!(stored.ide.as_deref(), Some("my editor"));
}

#[tokio::test]
async fn settings_mouse_bio_wide_wrapped_caret_wheel_and_done_preserve_text() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Bio));
    click(&mut app, Target::Bio);
    app.handle_input(
        "漢字 e\u{301} first line\nsecond line\n"
            .repeat(20)
            .as_bytes(),
    );
    app.resize(48, 14).unwrap();
    let buffer = paint(&app);
    let done = hits(&app)
        .into_iter()
        .find_map(|(rect, target)| (target == Target::Submit).then_some(rect))
        .unwrap();
    assert_eq!(buffer[(done.x - 1, done.y)].symbol(), " ");
    for x in done.right()..buffer.area.right() - 2 {
        assert_eq!(buffer[(x, done.y)].symbol(), " ");
    }
    let before = app.settings_modal_state.bio_input().lines().to_vec();
    let cursor = app.settings_modal_state.bio_input().cursor();
    let target = hits(&app)
        .iter()
        .find_map(|(_, target)| {
            matches!(target, Target::Caret(Field::Bio, _, _)).then_some(*target)
        })
        .unwrap();
    wheel(&mut app, target, false);
    assert_eq!(app.settings_modal_state.bio_input().lines(), before);
    assert_eq!(app.settings_modal_state.bio_input().cursor(), cursor);
    let target = hits_after_paint(&app)
        .iter()
        .find_map(|(_, target)| {
            matches!(target, Target::Caret(Field::Bio, _, _)).then_some(*target)
        })
        .unwrap();
    let Target::Caret(_, row, col) = target else {
        unreachable!()
    };
    click(&mut app, target);
    assert_eq!(app.settings_modal_state.bio_input().cursor(), (row, col));
    assert_eq!(app.settings_modal_state.bio_input().lines(), before);
    click(&mut app, Target::Submit);
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    assert!(!app.settings_modal_state.editing_bio());
    assert_eq!(
        app.settings_modal_state.draft().bio,
        before.join("\n").trim_end()
    );
    // Existing Bio Enter/Esc timing remains synchronous.
    app.handle_input(b"\r\x1b");
    settle(&mut app, |app| !app.settings_modal_state.editing_bio()).await;
}

#[tokio::test]
async fn settings_mouse_rss_waits_for_storage_and_preserves_selected_feed_identity() {
    let (db, mut app) = fixture().await;
    let client = db.db.get().await.unwrap();
    let original = RssFeed::create_for_user(&client, app.user_id, "http://127.0.0.1:1/original")
        .await
        .unwrap();
    app.settings_modal_state.open_from_profile(
        &Profile::load(&client, app.user_id).await.unwrap(),
        app.rail_modes(),
    );
    drop(client);
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 1).await;
    click(&mut app, Target::Tab(Tab::Feeds));
    click(&mut app, Target::AddFeed);
    app.handle_input(b"bad-url");
    click(&mut app, Target::Tab(Tab::Account));
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    assert!(app.settings_modal_state.editing_feed_url());
    assert_eq!(
        app.settings_modal_state.feed_url_input().lines(),
        &["bad-url"]
    );
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Feeds);
    app.handle_input(b"\x15http://127.0.0.1:1/new");
    click(&mut app, Target::Feed(original.id));
    assert!(app.settings_modal_state.mouse_save_pending());
    settle(&mut app, |app| {
        !app.settings_modal_state.mouse_save_pending()
    })
    .await;
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 2).await;
    assert!(!app.settings_modal_state.editing_feed_url());
    assert_eq!(
        app.settings_modal_state.feeds()[app.settings_modal_state.feed_index()].id,
        original.id
    );
    wheel(&mut app, Target::Feed(original.id), true);
    assert_eq!(
        app.settings_modal_state.feeds()[app.settings_modal_state.feed_index()].id,
        original.id
    );
    click(&mut app, Target::RemoveFeed);
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 1).await;
    click(&mut app, Target::RefreshFeeds);
}

#[tokio::test]
async fn settings_mouse_account_dialogs_require_typed_confirmation_and_block_pending_clicks() {
    let (db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Account));
    let buffer = paint(&app);
    assert!(text(&buffer).contains("IRC access token"));
    click(&mut app, Target::Account(AccountRow::LinkAccounts));
    click(&mut app, Target::GenerateCode);
    assert!(app.settings_modal_state.link_account_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.link_account_dialog().open());
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    click(&mut app, Target::Caret(Field::LinkCode, 0, 0));
    app.handle_input(b"invalid-code");
    click(&mut app, Target::LookupCode);
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .status()
            .is_some()
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::DeleteAccount));
    app.handle_input(b"wrong-name");
    click(&mut app, Target::ConfirmDelete);
    assert!(!app.settings_modal_state.delete_account_dialog().pending());
    assert!(
        app.settings_modal_state
            .delete_account_dialog()
            .status()
            .unwrap()
            .contains("does not match")
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::IrcToken));
    settle(&mut app, |app| {
        app.settings_modal_state
            .irc_token_dialog()
            .status()
            .is_some()
    })
    .await;
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Primary));
    assert!(app.settings_modal_state.irc_token_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.irc_token_dialog().open());
    settle(&mut app, |app| {
        app.settings_modal_state
            .irc_token_dialog()
            .revealed_token()
            .is_some()
    })
    .await;
    click(&mut app, Target::DismissToken);
    settle(&mut app, |app| {
        app.settings_modal_state.irc_token_dialog().has_token()
    })
    .await;
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Revoke));
    assert!(
        app.settings_modal_state
            .irc_token_dialog()
            .confirming_revoke()
    );
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Revoke));
    assert!(app.settings_modal_state.irc_token_dialog().pending());
    settle(&mut app, |app| {
        !app.settings_modal_state.irc_token_dialog().pending()
    })
    .await;
    assert!(!app.settings_modal_state.irc_token_dialog().has_token());
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::DeleteAccount));
    app.handle_input(b"mouse-user");
    click(&mut app, Target::ConfirmDelete);
    assert!(app.settings_modal_state.delete_account_dialog().pending());
    let client = db.db.get().await.unwrap();
    crate::test_helpers::wait_until(
        || async {
            late_core::models::user::User::get(&client, app.user_id)
                .await
                .unwrap()
                .is_none()
        },
        "disposable account to be deleted",
    )
    .await;
}

#[tokio::test]
async fn settings_mouse_keyboard_only_mode_and_non_left_events_do_not_activate() {
    let (_db, mut app) = fixture().await;
    app.interaction_mode = InteractionMode::Keyboard;
    click(&mut app, Target::Tab(Tab::Themes));
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    app.interaction_mode = InteractionMode::Hybrid;
    paint(&app);
    let rect = hits(&app)
        .iter()
        .find_map(|(rect, hit)| (*hit == Target::Tab(Tab::Themes)).then_some(*rect))
        .unwrap();
    for (button, suffix) in [(2, "M"), (0, "m"), (32, "M")] {
        app.handle_input(
            format!("\x1b[<{button};{};{}{suffix}", rect.x + 1, rect.y + 1).as_bytes(),
        );
        assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    }
    app.resize(48, 14).unwrap();
    app.handle_input(format!("\x1b[<0;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    app.handle_input(b"\t");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Bio);
}

#[tokio::test]
async fn settings_mouse_theme_search_stars_groups_and_wheel_do_not_apply_accidentally() {
    use super::state::ThemeTreeRow;
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Themes));
    let group = app
        .settings_modal_state
        .theme_tree_rows()
        .iter()
        .position(|row| matches!(row, ThemeTreeRow::Group { .. }))
        .unwrap();
    let before = app.settings_modal_state.theme_tree_rows().len();
    click(&mut app, Target::Theme(group));
    assert_ne!(app.settings_modal_state.theme_tree_rows().len(), before);
    click(&mut app, Target::Search);
    app.handle_input(b"paper");
    let row = app
        .settings_modal_state
        .theme_tree_rows()
        .iter()
        .position(|row| matches!(row, ThemeTreeRow::Theme { .. }))
        .unwrap();
    let theme = app.settings_modal_state.draft().theme_id.clone();
    let selected = app.settings_modal_state.theme_selected_row();
    wheel(&mut app, Target::Theme(row), true);
    assert_eq!(app.settings_modal_state.theme_selected_row(), selected);
    assert_eq!(app.settings_modal_state.draft().theme_id, theme);
    let favorites = app.settings_modal_state.draft().favorite_theme_ids.clone();
    click(&mut app, Target::Star(row));
    assert_ne!(
        app.settings_modal_state.draft().favorite_theme_ids,
        favorites
    );
    click(&mut app, Target::Theme(row));
    assert_eq!(app.settings_modal_state.draft().theme_id, theme);
    app.handle_input(b"\x15no-theme-matches-this");
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, hit)| matches!(hit, Target::Theme(_) | Target::Star(_)))
    );
}

#[tokio::test]
async fn settings_mouse_link_confirmation_choices_fields_and_submit_on_short_terminal() {
    use late_core::models::account_link;
    use late_core::models::user::User;
    let (db, mut app) = fixture().await;
    let other = create_test_user(&db.db, "peer-user").await;
    let client = db.db.get().await.unwrap();
    let (code, _) = account_link::create_code(&client, other.id).await.unwrap();
    click(&mut app, Target::Tab(Tab::Account));
    click(&mut app, Target::Account(AccountRow::LinkAccounts));
    click(&mut app, Target::Caret(Field::LinkCode, 0, 0));
    app.handle_input(code.as_bytes());
    click(&mut app, Target::LookupCode);
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    assert_eq!(
        app.settings_modal_state.link_account_dialog().step(),
        super::state::LinkAccountStep::Confirm
    );
    click(&mut app, Target::KeepAccount(false));
    assert!(
        !app.settings_modal_state
            .link_account_dialog()
            .keep_current()
    );
    click(&mut app, Target::KeepAccount(true));
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .keep_current()
    );
    click(&mut app, Target::ConfirmLink);
    assert!(!app.settings_modal_state.link_account_dialog().pending());
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .status()
            .unwrap()
            .contains("does not match")
    );
    click(&mut app, Target::Caret(Field::LinkConfirm, 0, 0));
    app.handle_input(b"mouse-user");
    app.resize(48, 14).unwrap();
    paint(&app);
    for _ in 0..8 {
        if hits(&app)
            .iter()
            .any(|(_, hit)| *hit == Target::ConfirmLink)
        {
            break;
        }
        let rect = hits(&app)
            .iter()
            .find_map(|(rect, hit)| {
                matches!(hit, Target::Caret(_, _, _) | Target::KeepAccount(_)).then_some(*rect)
            })
            .unwrap();
        app.handle_input(format!("\x1b[<65;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
        paint(&app);
    }
    click(&mut app, Target::ConfirmLink);
    assert!(app.settings_modal_state.link_account_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.link_account_dialog().open());
    crate::test_helpers::wait_until(
        || async { User::get(&client, other.id).await.unwrap().is_none() },
        "disposable peer to be linked",
    )
    .await;
}
