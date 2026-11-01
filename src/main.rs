#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod benchmark;
mod browser;
mod gif_viewer;
mod heif;
mod list_animation_viewer;
mod native;
mod platform;
mod playback;
mod prototype;
mod scan_report;
mod startup_trace;
mod thumbnails;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let os_args: Vec<_> = std::env::args_os().collect();
    let args: Vec<String> = os_args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let os_argument = |key: &str| {
        os_args
            .iter()
            .position(|s| s == key)
            .and_then(|i| os_args.get(i + 1))
            .map(std::path::PathBuf::from)
    };
    let launch_root = os_argument("--open");
    let instance_report = os_argument("--instance-report");
    #[cfg(windows)]
    if let Some(action) = args.get(1).map(String::as_str).filter(|s| {
        matches!(
            *s,
            "--register-context-menu"
                | "--unregister-context-menu"
                | "--register-test-menu"
                | "--unregister-test-menu"
        )
    }) {
        let test_scope = if action.contains("test-menu") {
            Some(
                args.get(2)
                    .ok_or(media_browser::ui_text!(msg_d74f0b618c8c))?
                    .as_str(),
            )
        } else {
            None
        };
        let exe = std::env::current_exe()?;
        if action.starts_with("--unregister") {
            println!(
                "{}",
                serde_json::to_string(&media_browser::shell_menu::unregister(&exe, test_scope)?)?
            );
        } else {
            println!(
                "{}",
                serde_json::to_string(&media_browser::shell_menu::register(&exe, test_scope)?)?
            );
        }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--scan-report") {
        return scan_report::run(&args).map_err(|e| e.to_string().into());
    }
    if args.get(1).map(String::as_str) == Some("--probe-image-size") {
        let path = std::path::Path::new(args.get(2).ok_or("Missing image path")?);
        let meta = thumbnails::probe_still_meta(path)?;
        let requested = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("heic") || e.eq_ignore_ascii_case("heif"))
        {
            Some(heif::Heif::new()?.dimensions_with_read_count(path)?.2)
        } else {
            None
        };
        println!(
            "{}",
            serde_json::json!({"width":meta.width,"height":meta.height,"heif_reader_requested_bytes":requested,"elapsed_ms":started.elapsed().as_millis()})
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--probe-video-header") {
        let path = std::path::Path::new(args.get(2).ok_or("Missing video path")?);
        println!(
            "{}",
            serde_json::json!({"header":media_browser::mp4_header::probe(path),"elapsed_ms":started.elapsed().as_secs_f64()*1000.})
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--decode-heic") {
        let decoder = heif::Heif::new()?;
        let img = decoder.decode(std::path::Path::new(
            args.get(2)
                .ok_or(media_browser::ui_text!(msg_506ced736802))?,
        ))?;
        let dimensions = img.dimensions();
        img.save(
            args.get(3)
                .ok_or(media_browser::ui_text!(msg_8688ff1ec18f))?,
        )?;
        println!(
            "{}",
            serde_json::json!({"decoder":decoder.version(),"width":dimensions.0,"height":dimensions.1,"elapsed_ms":started.elapsed().as_millis()})
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--probe-video") {
        return prototype::probe_video(std::path::Path::new(
            args.get(2)
                .ok_or(media_browser::ui_text!(msg_50792e78d2d9))?,
        ))
        .map_err(Into::into);
    }
    startup_trace::init(started, os_argument("--startup-trace"));
    startup_trace::mark("arguments_ready");
    let p0 = args
        .iter()
        .any(|s| matches!(s.as_str(), "--p0" | "--video" | "--heic" | "--smoke"));
    startup_trace::mark("before_instance");
    #[cfg(windows)]
    let instance = if p0
        || args.iter().any(|s| {
            matches!(
                s.as_str(),
                "--features-smoke"
                    | "--loading-smoke"
                    | "--browser-smoke"
                    | "--folder-smoke"
                    | "--detail-smoke"
                    | "--format-smoke"
                    | "--seek-benchmark"
                    | "--edit-smoke"
                    | "--cloud-smoke"
                    | "--input-smoke"
                    | "--playback-smoke"
                    | "--gif-smoke"
                    | "--animation-smoke"
                    | "--media-jump-smoke"
                    | "--empty-fold-smoke"
                    | "--watch-smoke"
                    | "--trash-smoke"
                    | "--benchmark-scroll"
            )
        }) {
        None
    } else {
        let scope = args
            .iter()
            .position(|s| s == "--instance-scope")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
            .unwrap_or("main");
        match media_browser::instance::enter(scope, launch_root.as_deref())? {
            media_browser::instance::Entry::Primary(primary) => Some(primary),
            media_browser::instance::Entry::Forwarded { server_pid } => {
                println!(
                    "{}",
                    serde_json::json!({"forwarded":true,"server_pid":server_pid,"client_pid":std::process::id()})
                );
                return Ok(());
            }
        }
    };
    startup_trace::mark("instance_ready");
    let startup_library = (!p0 && args.iter().any(|s| s == "--startup-overlap-library"))
        .then(|| browser::StartupLibrary::start(&args));
    let args_minimize = args.iter().any(|s| s == "--instance-minimize-after-open");
    let controlled_4k = args.iter().any(|s| s == "--benchmark-scroll")
        && args.iter().any(|s| s == "--benchmark-surface-4k");
    let fullscreen = args.iter().any(|s| s == "--fullscreen") && !controlled_4k;
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_icon(media_browser::branding::icon())
        .with_fullscreen(fullscreen)
        .with_decorations(p0)
        .with_title(if p0 {
            media_browser::ui_text!(msg_8a4eb8b2005a)
        } else {
            media_browser::ui_text!(msg_81793ece95dd)
        });
    // An explicit inner size can override the monitor-sized framebuffer at creation
    // even when winit reports fullscreen=true. Only apply the normal window size.
    if controlled_4k {
        // Opt-in native render workload, even when the desktop is smaller.
        // Actual framebuffer dimensions still have to pass the benchmark verifier.
        viewport = viewport
            .with_inner_size([640., 360.])
            .with_clamp_size_to_monitor_size(false);
    } else if !fullscreen {
        viewport = viewport.with_inner_size([1100., 760.]);
    }
    if !p0
        && !fullscreen
        && !args
            .iter()
            .any(|a| matches!(a.as_str(), "--benchmark-scroll" | "--seek-benchmark"))
    {
        viewport = viewport.with_min_inner_size(media_browser::window_recovery::MINIMUM);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    startup_trace::mark("before_run_native");
    let result = eframe::run_native(
        media_browser::ui_text!(msg_6155b88621d2),
        options,
        Box::new(move |cc| {
            startup_trace::mark("creation_context_ready");
            let app: Box<dyn eframe::App> = if p0 {
                Box::new(prototype::Prototype::new(cc, args, started))
            } else {
                let mut browser = browser::Browser::new(cc, args, started, startup_library);
                #[cfg(windows)]
                if let Some(primary) = instance {
                    browser.attach_instance(primary, launch_root, instance_report, args_minimize);
                }
                startup_trace::mark("instance_attached");
                Box::new(browser)
            };
            startup_trace::mark("app_creator_done");
            Ok(app)
        }),
    );
    startup_trace::mark("native_loop_returned");
    startup_trace::finish()?;
    result.map_err(|e| e.to_string().into())
}
mod file_clipboard;
