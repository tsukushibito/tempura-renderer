mod temp_renderer;

use raw_window_handle::HasRawWindowHandle;
use temp_renderer::Renderer;

pub fn test(window_handle: &dyn HasRawWindowHandle) {
    let renderer = Renderer::new(window_handle);
}
