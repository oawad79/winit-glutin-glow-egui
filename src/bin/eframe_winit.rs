use std::error::Error;
use std::sync::Arc;

use eframe::egui;
use egui::mutex::Mutex;
use glow::HasContext;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("eframe with Custom OpenGL Rendering"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "eframe glow app",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
    .map_err(Into::into)
}

struct MyApp {
    // Wrap OpenGL resources in Arc<Mutex<>> so they can be shared with the paint callback
    triangle_renderer: Arc<Mutex<TriangleRenderer>>,

    // UI state
    show_color_picker: bool,
    color: [f32; 3],
}

struct TriangleRenderer {
    program: glow::Program,
    vertex_array: glow::VertexArray,
}

impl TriangleRenderer {
    fn new(gl: &glow::Context) -> Self {
        unsafe {
            let vertex_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertex_array));

            let program = gl.create_program().expect("Cannot create program");

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

            Self {
                program,
                vertex_array,
            }
        }
    }

    fn paint(&self, gl: &glow::Context, color: [f32; 3]) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vertex_array));

            let color_location = gl.get_uniform_location(self.program, "u_color");
            gl.uniform_3_f32(color_location.as_ref(), color[0], color[1], color[2]);

            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc.gl.as_ref().expect("Failed to get glow context");
        let triangle_renderer = Arc::new(Mutex::new(TriangleRenderer::new(gl)));

        Self {
            triangle_renderer,
            show_color_picker: false,
            color: [1.0, 0.5, 0.2],
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Handle keyboard input
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Space) {
                self.show_color_picker = !self.show_color_picker;
            }
        });

        // IMPORTANT: Use CentralPanel to fill the entire window with our custom rendering
        // The custom paint callback allows us to inject OpenGL rendering BEFORE egui UI
        egui::CentralPanel::default().show(ctx, |ui| {
            // Create a custom paint callback that renders our triangle
            let triangle_renderer = self.triangle_renderer.clone();
            let color = self.color;

            let callback = egui::PaintCallback {
                rect: ui.max_rect(),
                callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                    let renderer = triangle_renderer.lock();
                    renderer.paint(painter.gl(), color);
                })),
            };

            ui.painter().add(callback);
        });

        // Show egui UI on top of the triangle
        if self.show_color_picker {
            egui::Window::new("Color Picker")
                .default_size([300.0, 200.0])
                .open(&mut self.show_color_picker)
                .show(ctx, |ui| {
                    ui.heading("Triangle Color");
                    ui.separator();

                    ui.label("Red:");
                    ui.add(egui::Slider::new(&mut self.color[0], 0.0..=1.0));

                    ui.label("Green:");
                    ui.add(egui::Slider::new(&mut self.color[1], 0.0..=1.0));

                    ui.label("Blue:");
                    ui.add(egui::Slider::new(&mut self.color[2], 0.0..=1.0));

                    ui.separator();
                    ui.label("Press SPACE to toggle this window");
                });
        }

        // Request continuous repainting
        ctx.request_repaint();
    }
}
