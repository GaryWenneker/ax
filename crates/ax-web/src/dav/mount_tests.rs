use super::*;
use axum::http::HeaderValue;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.insert(*k, HeaderValue::from_str(v).unwrap());
    }
    h
}

#[test]
fn b6_name_rules() {
    assert!(valid_name("ax"));
    assert!(valid_name("My Vault_1-a"));
    assert!(valid_name(&"a".repeat(32)));
    assert!(!valid_name(""));
    assert!(!valid_name(&"a".repeat(33)));
    assert!(!valid_name("ax;rm"));
    assert!(!valid_name("../ax"));
    assert!(!valid_name("a\"b"));
}

#[test]
fn b8_free_letter_picks_highest_unused() {
    assert_eq!(free_letter(&['C', 'D', 'Z']), Some('Y'));
    assert_eq!(free_letter(&[]), Some('Z'));
    assert_eq!(free_letter(&['z']), Some('Y'));
    let all: Vec<char> = ('D'..='Z').collect();
    assert_eq!(free_letter(&all), None);
    let all_but_d: Vec<char> = ('E'..='Z').collect();
    assert_eq!(free_letter(&all_but_d), Some('D'));
}

#[test]
fn url_uses_ax_path() {
    assert_eq!(dav_url(7070), "http://127.0.0.1:7070/ax/");
}

#[test]
fn b7_mac_commands() {
    assert_eq!(
        mount_commands(Os::Mac, 7070, "ax", "/Users/g/ax", true),
        vec![s(&[
            "/sbin/mount_webdav",
            "-S",
            "-v",
            "ax",
            "http://127.0.0.1:7070/ax/",
            "/Users/g/ax"
        ])]
    );
    assert_eq!(
        unmount_commands(Os::Mac, 7070, "/Users/g/ax"),
        vec![s(&["/sbin/umount", "/Users/g/ax"])]
    );
}

#[test]
fn b7_windows_commands() {
    assert_eq!(
        mount_commands(Os::Windows, 7070, "ax", "Y:", true),
        vec![
            s(&[
                "net",
                "use",
                "Y:",
                r"\\127.0.0.1@7070\ax",
                "/persistent:yes"
            ]),
            s(&[
                "reg",
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\MountPoints2\##127.0.0.1@7070#ax",
                "/v",
                "_LabelFromReg",
                "/t",
                "REG_SZ",
                "/d",
                "ax",
                "/f"
            ]),
        ]
    );
    assert_eq!(
        unmount_commands(Os::Windows, 7070, "Y:"),
        vec![s(&["net", "use", "Y:", "/delete", "/y"])]
    );
}

#[test]
fn b7_windows_without_autostart_is_not_persistent() {
    assert_eq!(
        mount_commands(Os::Windows, 7070, "ax", "D:", false)[0],
        s(&["net", "use", "D:", r"\\127.0.0.1@7070\ax", "/persistent:no"])
    );
}

#[test]
fn b11_open_commands() {
    assert_eq!(
        open_command(Os::Mac, "/Users/g/ax"),
        s(&["open", "/Users/g/ax"])
    );
    assert_eq!(open_command(Os::Windows, "Y:"), s(&["explorer", r"Y:\"]));
    assert_eq!(
        open_command(Os::Linux, "/run/x"),
        s(&["xdg-open", "/run/x"])
    );
}

#[test]
fn b7_linux_commands() {
    assert_eq!(
        mount_commands(Os::Linux, 7071, "ax", "", false),
        vec![s(&["gio", "mount", "dav://127.0.0.1:7071/ax/"])]
    );
    assert_eq!(
        unmount_commands(Os::Linux, 7071, ""),
        vec![s(&["gio", "mount", "-u", "dav://127.0.0.1:7071/ax/"])]
    );
}

#[test]
fn n1_loopback_only() {
    assert!(allowed(&headers(&[("host", "127.0.0.1:7070")]), false));
    assert!(allowed(&headers(&[("host", "localhost")]), false));
    assert!(allowed(&headers(&[("host", "[::1]:7070")]), false));
    assert!(allowed(
        &headers(&[
            ("host", "127.0.0.1:7070"),
            ("origin", "http://localhost:5173")
        ]),
        false
    ));
    assert!(!allowed(&headers(&[("host", "127.0.0.1:7070")]), true));
    assert!(!allowed(&headers(&[]), false));
    assert!(!allowed(&headers(&[("host", "192.168.1.5:7070")]), false));
    assert!(!allowed(&headers(&[("host", "127.0.0.1.evil.com")]), false));
    assert!(!allowed(
        &headers(&[("host", "127.0.0.1:7070"), ("origin", "https://evil.com")]),
        false
    ));
    assert!(!allowed(
        &headers(&[
            ("host", "127.0.0.1:7070"),
            ("origin", "http://localhost.evil.com")
        ]),
        false
    ));
}
