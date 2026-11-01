pub mod animation;
pub mod branding;
pub mod color;
pub mod domain;
pub mod empty_folders;
pub mod favorites;
pub mod index;
pub mod input;
pub mod layout;
pub mod library;
pub mod media_path;
pub mod mp4_header;
pub mod photo_rotate;
pub mod places;
pub mod playback_control;
pub mod prepare;
pub mod scanner;
pub mod shortcuts;
pub mod subtitles;
pub mod telemetry;
pub mod trash;
pub mod ui_text;
pub mod video_prefetch;
pub mod view;
pub mod watch;

#[cfg(windows)]
pub mod instance;

pub mod presentation;
#[cfg(windows)]
pub mod shell_menu;

pub mod index_recovery;
pub mod window_recovery;

pub mod long_png;
pub mod media_jump;

pub mod gif_previews;
