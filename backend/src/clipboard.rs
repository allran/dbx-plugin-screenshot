use arboard::{Clipboard, ImageData};
use dbx_plugin_sdk::PluginError;

use crate::media::decode_image_file;
use crate::model::ScreenshotItem;
use crate::util::data_dir;

pub fn write_item_to_clipboard(item: &ScreenshotItem) -> Result<(), PluginError> {
    let rel = item
        .media_file
        .as_ref()
        .ok_or_else(|| PluginError::new(-32000, "Image file missing"))?;
    let (width, height, rgba) = decode_image_file(&data_dir().join(rel))?;
    let mut clipboard = Clipboard::new()
        .map_err(|err| PluginError::new(-32000, format!("Clipboard unavailable: {err}")))?;
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: rgba.into(),
        })
        .map_err(|err| PluginError::new(-32000, format!("Failed to write image: {err}")))
}
