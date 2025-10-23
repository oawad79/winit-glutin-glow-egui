use std::collections::HashMap;
use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::raw_window_handle::{DisplayHandle, HasDisplayHandle};
use winit::window::{Window, WindowId};

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = Application::new(&event_loop)?;
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct Application {
    context: Context<DisplayHandle<'static>>,
    windows: HashMap<WindowId, WindowState>,
}

struct WindowState {
    window: Arc<Window>,
    surface: Surface<DisplayHandle<'static>, Arc<Window>>,
}

impl Application {
    fn new(event_loop: &EventLoop<()>) -> Result<Self, Box<dyn Error>> {
        let context = Context::new(unsafe {
            std::mem::transmute::<DisplayHandle<'_>, DisplayHandle<'static>>(
                event_loop.display_handle()?,
            )
        })?;

        Ok(Self {
            context,
            windows: HashMap::new(),
        })
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let window = Arc::new(
            event_loop.create_window(Window::default_attributes().with_title("Simple Window"))?,
        );

        let surface = Surface::new(&self.context, Arc::clone(&window))?;
        let window_id = window.id();

        self.windows
            .insert(window_id, WindowState { window, surface });
        Ok(())
    }
}

impl ApplicationHandler for Application {
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let window_state = match self.windows.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&window_id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                let size = window_state.window.inner_size();
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    window_state.surface.resize(width, height).unwrap();

                    let mut buffer = window_state.surface.buffer_mut().unwrap();
                    // Fill with a simple blue color
                    for pixel in buffer.iter_mut() {
                        *pixel = 0xFF0066CC; // ARGB format: blue
                    }

                    window_state.window.pre_present_notify();
                    buffer.present().unwrap();
                }
            }
            WindowEvent::Resized(size) => {
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    window_state.surface.resize(width, height).unwrap();
                    window_state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop)
            .expect("Failed to create window");
    }
}
