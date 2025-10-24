use std::collections::HashMap;
use std::error::Error;
use std::num::NonZeroU32;

use glow::*;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin_winit::{DisplayBuilder, GlWindow};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    let mut app = Application::new();
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct Application {
    template: Option<glutin::config::Config>,
    display: Option<glutin::display::Display>,
    windows: HashMap<WindowId, WindowState>,
}

struct WindowState {
    window: Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    gl: Arc<glow::Context>,
    program: glow::Program,
    vertex_array: glow::VertexArray,

    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    egui_painter: egui_glow::Painter,

    show_color_picker: bool,
    color: [f32; 3],
}

impl Application {
    fn new() -> Self {
        Self {
            template: None,
            display: None,
            windows: HashMap::new(),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let window_attributes = Window::default_attributes()
            .with_title("Glow OpenGL Window with egui - Press SPACE for color picker")
            .with_inner_size(PhysicalSize::new(800, 600));

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(false);

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|accum, config| {
                        if config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .unwrap()
            })
            .unwrap();

        let raw_window_handle = window
            .as_ref()
            .and_then(|window| window.window_handle().ok().map(|h| h.as_raw()));
        let gl_display = gl_config.display();
        let window = window.unwrap();

        let attrs = window.build_surface_attributes(Default::default()).unwrap();
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &attrs)
                .unwrap()
        };

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version {
                major: 4,
                minor: 1,
            })))
            .build(raw_window_handle);

        let gl_context = unsafe { gl_display.create_context(&gl_config, &context_attributes)? };

        let gl_context = gl_context.make_current(&gl_surface)?;

        let gl = Arc::new(unsafe {
            glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s).cast())
        });

        // Create OpenGL resources for rendering a simple triangle
        let (program, vertex_array) = unsafe {
            let vertex_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertex_array));

            let program = gl.create_program().expect("Cannot create program");

            // Simple shaders that render a triangle with a uniform color
            let (vertex_shader_source, fragment_shader_source) = (
                r#"const vec2 verts[3] = vec2[3](
                vec2(0.5f, 1.0f),
                vec2(0.0f, 0.0f),
                vec2(1.0f, 0.0f)
            );
            out vec2 vert;
            void main() {
                vert = verts[gl_VertexID];
                gl_Position = vec4(vert - 0.5, 0.0, 1.0);
            }"#,
                r#"precision mediump float;
            uniform vec3 u_color;
            in vec2 vert;
            out vec4 color;
            void main() {
                color = vec4(u_color, 1.0);
            }"#,
            );

            let shader_sources = [
                (glow::VERTEX_SHADER, vertex_shader_source),
                (glow::FRAGMENT_SHADER, fragment_shader_source),
            ];

            let mut shaders = Vec::with_capacity(shader_sources.len());

            for (shader_type, shader_source) in shader_sources.iter() {
                let shader = gl
                    .create_shader(*shader_type)
                    .expect("Cannot create shader");
                gl.shader_source(shader, &format!("{}\n{}", "#version 410", shader_source));
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    panic!("{}", gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }

            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("{}", gl.get_program_info_log(program));
            }

            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }

            (program, vertex_array)
        };

        // Initialize egui context and state
        let egui_ctx = egui::Context::default();
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        // Create egui painter for rendering egui with glow
        let egui_painter = egui_glow::Painter::new(gl.clone(), "", None, false).unwrap();

        // Request focus for the window to ensure keyboard events are received
        window.focus_window();

        let window_id = window.id();
        let window_state = WindowState {
            window,
            gl_context,
            gl_surface,
            gl,
            program,
            vertex_array,
            egui_ctx,
            egui_winit,
            egui_painter,
            show_color_picker: false,
            color: [1.0, 0.5, 0.2],
        };

        self.windows.insert(window_id, window_state);
        self.display = Some(gl_display);
        self.template = Some(gl_config);

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

        // IMPORTANT: Handle keyboard input BEFORE passing to egui
        // This allows us to intercept keys for application-level shortcuts
        // Issue: Initially keyboard events weren't being received because we weren't
        // checking for them explicitly and the window might not have had focus
        match &event {
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    if event.physical_key
                        == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Space)
                    {
                        window_state.show_color_picker = !window_state.show_color_picker;
                        window_state.window.request_redraw();
                    }
                }
            }
            _ => {}
        }

        // Pass event to egui for UI interaction
        let event_response = window_state
            .egui_winit
            .on_window_event(&window_state.window, &event);
        if event_response.repaint {
            window_state.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&window_id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => unsafe {
                let size = window_state.window.inner_size();

                // Clear and draw triangle with custom color
                window_state
                    .gl
                    .viewport(0, 0, size.width as i32, size.height as i32);
                window_state.gl.clear_color(0.1, 0.2, 0.3, 1.0);
                window_state.gl.clear(glow::COLOR_BUFFER_BIT);

                window_state.gl.use_program(Some(window_state.program));
                window_state
                    .gl
                    .bind_vertex_array(Some(window_state.vertex_array));

                // Set the triangle color from our state
                let color_location = window_state
                    .gl
                    .get_uniform_location(window_state.program, "u_color");
                window_state.gl.uniform_3_f32(
                    color_location.as_ref(),
                    window_state.color[0],
                    window_state.color[1],
                    window_state.color[2],
                );

                window_state.gl.draw_arrays(glow::TRIANGLES, 0, 3);

                // Prepare egui frame
                let raw_input = window_state
                    .egui_winit
                    .take_egui_input(&window_state.window);
                let show_color_picker = &mut window_state.show_color_picker;
                let color = &mut window_state.color;

                // Run egui UI code
                let full_output = window_state.egui_ctx.run(raw_input, |ctx| {
                    if *show_color_picker {
                        egui::Window::new("Color Picker")
                            .default_size([300.0, 200.0])
                            .open(show_color_picker)
                            .show(ctx, |ui| {
                                ui.heading("Triangle Color");
                                ui.separator();

                                ui.label("Red:");
                                ui.add(egui::Slider::new(&mut color[0], 0.0..=1.0));

                                ui.label("Green:");
                                ui.add(egui::Slider::new(&mut color[1], 0.0..=1.0));

                                ui.label("Blue:");
                                ui.add(egui::Slider::new(&mut color[2], 0.0..=1.0));

                                ui.separator();
                                ui.label("Press SPACE to toggle this window");
                            });
                    }
                });

                // Handle platform-specific output (cursor changes, clipboard, etc.)
                window_state
                    .egui_winit
                    .handle_platform_output(&window_state.window, full_output.platform_output);

                // CRITICAL: Handle texture updates from egui
                // Issue: Initially we got "Failed to find texture Managed(0)" warnings
                // because we weren't uploading egui's font atlas and other textures to the GPU.
                // egui generates texture deltas (new textures or updates) that must be uploaded
                // before rendering, otherwise egui can't render text or images.
                for (id, image_delta) in &full_output.textures_delta.set {
                    window_state.egui_painter.set_texture(*id, image_delta);
                }

                // Tessellate egui's shapes into triangles for rendering
                let clipped_primitives = window_state
                    .egui_ctx
                    .tessellate(full_output.shapes, full_output.pixels_per_point);

                // Render egui on top of our OpenGL content
                window_state.egui_painter.paint_primitives(
                    [size.width, size.height],
                    full_output.pixels_per_point,
                    &clipped_primitives,
                );

                // Free textures that are no longer needed
                for id in &full_output.textures_delta.free {
                    window_state.egui_painter.free_texture(*id);
                }

                // Present the rendered frame
                window_state
                    .gl_surface
                    .swap_buffers(&window_state.gl_context)
                    .unwrap();
            },
            WindowEvent::Resized(size) => {
                if size.width != 0 && size.height != 0 {
                    window_state.gl_surface.resize(
                        &window_state.gl_context,
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
                    window_state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            self.create_window(event_loop)
                .expect("Failed to create window");
        }
    }

    // Continuously request redraws to keep the application responsive
    // Issue: Without this, the window would only redraw on explicit events,
    // making the UI feel unresponsive and animations wouldn't work smoothly
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        for window_state in self.windows.values() {
            window_state.window.request_redraw();
        }
    }
}
