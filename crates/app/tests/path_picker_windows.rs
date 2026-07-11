//! Windows drive-root navigation through the real LocalHost picker.

#![cfg(target_os = "windows")]

use std::path::PathBuf;
use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, wait_for};

mod common;

#[gpui::test]
async fn drive_root_parent_lists_drives_and_can_reenter_a_drive(cx: &mut TestAppContext) {
    let (workspace, window) = build_workspace(cx, None);
    window.update(|window, cx| {
        workspace.update(cx, |view, cx| view.open_repo_picker_for_test(window, cx));
    });
    let picker = window
        .update(|_window, cx| workspace.read(cx).path_picker_for_test().cloned())
        .expect("picker should be open");

    let current = std::env::current_dir().expect("current directory");
    let drive_root = current
        .ancestors()
        .last()
        .expect("Windows path should have a drive root")
        .to_path_buf();
    let drive_query = format!(
        "{}\\",
        drive_root.display().to_string().trim_end_matches('\\')
    );

    window.update(|window, cx| {
        picker.update(cx, |picker, cx| {
            picker.set_query(&drive_query, window, cx);
        });
    });
    wait_for(
        window,
        |cx| cx.read(|cx| !picker.read(cx).is_loading()),
        Duration::from_secs(5),
    );

    window.update(|window, cx| {
        picker.update(cx, |picker, cx| picker.go_parent_for_test(window, cx));
    });

    let (query, drives) = window.update(|_window, cx| {
        let picker = picker.read(cx);
        (picker.query(cx), picker.filtered_names())
    });
    assert_eq!(query, "", "drive root parent should be This PC");
    assert!(!drives.is_empty(), "This PC should list logical drives");
    assert!(
        drives.iter().all(|drive| {
            drive.len() == 2 && drive.as_bytes()[0].is_ascii_alphabetic() && drive.ends_with(':')
        }),
        "unexpected drive entries: {drives:?}"
    );

    window.update(|window, cx| {
        picker.update(cx, |picker, cx| picker.enter_selected_for_test(window, cx));
    });
    let selected_root = window.update(|_window, cx| picker.read(cx).query(cx));
    assert_eq!(
        PathBuf::from(&selected_root).ancestors().count(),
        1,
        "selecting a drive should enter its root: {selected_root}"
    );
    assert!(selected_root.ends_with('\\'));

    shutdown_workspace(window, &workspace);
}
