use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use ash_sample;

fn main() {
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Example")
        // .with_inner_size(winit::dpi::LogicalSize::new(128.0, 128.0))
        .build(&event_loop)
        .unwrap();
    ash_sample::test(&window);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, window_id } => match (event, window_id) {
                (WindowEvent::CloseRequested, _window_id) if _window_id == window.id() => {
                    *control_flow = ControlFlow::Exit
                }
                _ => (),
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => (),
        }
    });
}
