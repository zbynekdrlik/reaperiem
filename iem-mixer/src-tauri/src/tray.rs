//! System tray icon and menu management

use iem_core::BandMember;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Icon size in pixels
const ICON_SIZE: u32 = 16;

/// Set up the tray icon with menu
pub fn setup_tray(
    app: &AppHandle,
    port: u16,
    members: &[BandMember],
) -> Result<(), Box<dyn std::error::Error>> {
    // Display full version with git hash for unique deploy identification
    let version_label = format!("IEM Mixer v{}", iem_core::full_version());
    let version_item = MenuItem::with_id(app, "version", &version_label, false, None::<&str>)?;

    let separator1 = PredefinedMenuItem::separator(app)?;

    // Member quick access submenu - dynamically populated
    let members_menu = Submenu::with_id(app, "members", "Open Mixer", true)?;

    if members.is_empty() {
        let placeholder = MenuItem::with_id(
            app,
            "no_members",
            "No members configured",
            false,
            None::<&str>,
        )?;
        members_menu.append(&placeholder)?;
    } else {
        for member in members {
            let item_id = format!("member_{}", member.id());
            let item = MenuItem::with_id(app, &item_id, &member.name, true, None::<&str>)?;
            members_menu.append(&item)?;
        }
    }

    let separator2 = PredefinedMenuItem::separator(app)?;

    let dashboard_item = MenuItem::with_id(app, "dashboard", "Open Dashboard", true, None::<&str>)?;

    let remote_url = get_remote_url(port);
    let remote_item = MenuItem::with_id(
        app,
        "remote",
        format!("Remote: {}", remote_url),
        true,
        None::<&str>,
    )?;

    // Copy URL to clipboard option
    let copy_url_item = MenuItem::with_id(app, "copy_url", "Copy Remote URL", true, None::<&str>)?;

    let separator3 = PredefinedMenuItem::separator(app)?;

    let quit_item = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &version_item,
            &separator1,
            &members_menu,
            &separator2,
            &dashboard_item,
            &remote_item,
            &copy_url_item,
            &separator3,
            &quit_item,
        ],
    )?;

    let icon = make_tray_icon();

    let port_copy = port;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("IEM Mixer")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "dashboard" => {
                    open_dashboard(app, port_copy);
                }
                "remote" => {
                    open_remote_in_browser(port_copy);
                }
                "copy_url" => {
                    copy_remote_url_to_clipboard(app, port_copy);
                }
                "quit" => {
                    tracing::info!("Exit requested from tray");
                    app.exit(0);
                }
                _ => {
                    // Check if it's a member ID
                    if id.starts_with("member_") {
                        let member = &id[7..];
                        open_member_mixer(app, port_copy, member);
                    }
                }
            }
        })
        .on_tray_icon_event(move |tray, event| {
            // Left-click opens dashboard
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Open the dashboard in the main window
fn open_dashboard(app: &AppHandle, port: u16) {
    tracing::info!("Opening dashboard");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.navigate(format!("http://localhost:{}", port).parse().unwrap());
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Open a specific member's mixer
fn open_member_mixer(app: &AppHandle, port: u16, member: &str) {
    tracing::info!(member = %member, "Opening mixer for member");
    if let Some(window) = app.get_webview_window("main") {
        let url = format!("http://localhost:{}/{}", port, member);
        let _ = window.navigate(url.parse().unwrap());
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Get the remote access URL (IP-based for LAN access)
fn get_remote_url(port: u16) -> String {
    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    if port == 80 {
        format!("http://{}", ip)
    } else {
        format!("http://{}:{}", ip, port)
    }
}

/// Open the remote URL in the default browser
fn open_remote_in_browser(port: u16) {
    let url = get_remote_url(port);
    tracing::info!("Opening remote URL in browser: {}", url);
    let _ = open::that(&url);
}

/// Copy remote URL to clipboard
fn copy_remote_url_to_clipboard(app: &AppHandle, port: u16) {
    let url = get_remote_url(port);
    tracing::info!("Copying remote URL to clipboard: {}", url);

    // Use Tauri's clipboard plugin if available, otherwise log
    // For now, just show a notification
    if let Some(window) = app.get_webview_window("main") {
        let js = format!(
            "navigator.clipboard.writeText('{}').then(() => console.log('URL copied'))",
            url
        );
        let _ = window.eval(&js);
    }
}

/// Create the tray icon (headphones icon)
fn make_tray_icon() -> Image<'static> {
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    // Draw a simple headphones shape
    let color = (0x4Au8, 0x9Eu8, 0xFFu8); // Accent blue

    // Left ear cup (circle)
    draw_circle(&mut rgba, 4, 10, 3, color);

    // Right ear cup (circle)
    draw_circle(&mut rgba, 12, 10, 3, color);

    // Headband (arc at top)
    for x in 3..=13 {
        let y = if x < 5 || x > 11 {
            4
        } else if x < 7 || x > 9 {
            3
        } else {
            2
        };
        set_pixel(&mut rgba, x, y, color);
    }

    // Connect band to cups
    set_pixel(&mut rgba, 3, 5, color);
    set_pixel(&mut rgba, 3, 6, color);
    set_pixel(&mut rgba, 3, 7, color);
    set_pixel(&mut rgba, 13, 5, color);
    set_pixel(&mut rgba, 13, 6, color);
    set_pixel(&mut rgba, 13, 7, color);

    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

fn set_pixel(rgba: &mut [u8], x: u32, y: u32, color: (u8, u8, u8)) {
    if x < ICON_SIZE && y < ICON_SIZE {
        let idx = ((y * ICON_SIZE + x) * 4) as usize;
        rgba[idx] = color.0;
        rgba[idx + 1] = color.1;
        rgba[idx + 2] = color.2;
        rgba[idx + 3] = 255;
    }
}

fn draw_circle(rgba: &mut [u8], cx: u32, cy: u32, r: u32, color: (u8, u8, u8)) {
    for dy in 0..=r {
        for dx in 0..=r {
            if dx * dx + dy * dy <= r * r {
                set_pixel(rgba, cx + dx, cy + dy, color);
                set_pixel(rgba, cx + dx, cy.saturating_sub(dy), color);
                set_pixel(rgba, cx.saturating_sub(dx), cy + dy, color);
                set_pixel(rgba, cx.saturating_sub(dx), cy.saturating_sub(dy), color);
            }
        }
    }
}
