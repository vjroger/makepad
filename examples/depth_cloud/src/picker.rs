//! Native file dialogs for the video and the depth model, so nothing has to
//! be typed on a command line. Answers arrive later as `FileDialogAction`s.

use makepad_widgets::makepad_platform::file_dialogs::{FileDialog, FileDialogAction};
use makepad_widgets::*;

pub const PICK_VIDEO: LiveId = live_id!(depth_cloud_pick_video);
pub const PICK_MODEL: LiveId = live_id!(depth_cloud_pick_model);

pub const VIDEO_EXTENSIONS: [&str; 9] = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "mpg", "mpeg"];
pub const MODEL_EXTENSIONS: [&str; 3] = ["pth", "pt", "safetensors"];

pub fn pick_video(cx: &mut Cx) {
    open(cx, PICK_VIDEO, "Open video", "Video", &VIDEO_EXTENSIONS);
}

pub fn pick_model(cx: &mut Cx) {
    open(cx, PICK_MODEL, "Open depth model", "Depth model", &MODEL_EXTENSIONS);
}

fn open(cx: &mut Cx, id: LiveId, title: &str, kind: &str, extensions: &[&str]) {
    let dialog = FileDialog::new()
        .set_id(id)
        .set_title(title.to_string())
        .add_filter(kind.to_string(), extensions.iter().map(|e| e.to_string()).collect())
        .add_filter("All Files".to_string(), vec!["*".to_string()]);
    cx.open_select_file_dialog(dialog);
}

/// The chosen path when `action` is dialog `id` answering with one.
pub fn picked(action: &Action, id: LiveId) -> Option<String> {
    let picked = action.downcast_ref::<FileDialogAction>()?;
    if picked.id() != id {
        return None;
    }
    Some(picked.path()?.to_string_lossy().into_owned())
}

/// True when `path` looks like model weights rather than a video.
pub fn is_model_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    MODEL_EXTENSIONS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}
