use glutin::{
    self,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, PossiblyCurrent,
};

pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

fn main() {
    let el = EventLoop::new();
    let wb = WindowBuilder::new()
        .with_title("Hello world!")
        .with_inner_size(glutin::dpi::LogicalSize::new(640.0, 480.0))
        .with_transparent(true);
    let windowed_context = ContextBuilder::new().build_windowed(wb, &el).unwrap();

    // let _windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };

    let gl = load(&windowed_context);

    println!(
        "Pixel format of the window's GL context: {:?}",
        windowed_context.get_pixel_format()
    );

    el.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = glutin::event_loop::ControlFlow::Exit,
            Event::RedrawRequested(_) => {
                draw_frame(&gl, [1.0, 0.5, 0.7, 0.3]);
                windowed_context.swap_buffers().unwrap();
            }
            _ => (),
        }
    });
}

fn load(gl_context: &glutin::Context<PossiblyCurrent>) -> gl::Gl {
    let gl = gl::Gl::load_with(|ptr| gl_context.get_proc_address(ptr) as *const _);

    let version = unsafe {
        let data = std::ffi::CStr::from_ptr(gl.GetString(gl::VERSION) as *const _)
            .to_bytes()
            .to_vec();
        String::from_utf8(data).unwrap()
    };
    println!("OpenGL version {}", version);

    unsafe {
        let vs = gl.CreateShader(gl::VERTEX_SHADER);
        gl.ShaderSource(
            vs,
            1,
            [VS_SRC.as_ptr() as *const _].as_ptr(),
            std::ptr::null(),
        );
        gl.CompileShader(vs);

        let fs = gl.CreateShader(gl::FRAGMENT_SHADER);
        gl.ShaderSource(
            fs,
            1,
            [FS_SRC.as_ptr() as *const _].as_ptr(),
            std::ptr::null(),
        );
        gl.CompileShader(fs);

        let program = gl.CreateProgram();
        gl.AttachShader(program, vs);
        gl.AttachShader(program, fs);
        gl.LinkProgram(program);
        gl.UseProgram(program);

        let mut vb = std::mem::zeroed();
        gl.GenBuffers(1, &mut vb);
        gl.BindBuffer(gl::ARRAY_BUFFER, vb);
        gl.BufferData(
            gl::ARRAY_BUFFER,
            (VERTEX_DATA.len() * std::mem::size_of::<f32>()) as gl::types::GLsizeiptr,
            VERTEX_DATA.as_ptr() as *const _,
            gl::STATIC_DRAW,
        );

        if gl.BindVertexArray.is_loaded() {
            let mut vao = std::mem::zeroed();
            gl.GenVertexArrays(1, &mut vao);
            gl.BindVertexArray(vao);
        }

        let pos_attrib = gl.GetAttribLocation(program, b"position\0".as_ptr() as *const _);
        let color_attrib = gl.GetAttribLocation(program, b"color\0".as_ptr() as *const _);
        gl.VertexAttribPointer(
            pos_attrib as gl::types::GLuint,
            2,
            gl::FLOAT,
            0,
            5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
            std::ptr::null(),
        );
        gl.VertexAttribPointer(
            color_attrib as gl::types::GLuint,
            3,
            gl::FLOAT,
            0,
            5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
            (2 * std::mem::size_of::<f32>()) as *const () as *const _,
        );
        gl.EnableVertexAttribArray(pos_attrib as gl::types::GLuint);
        gl.EnableVertexAttribArray(color_attrib as gl::types::GLuint);
    }

    gl
}

pub fn draw_frame(gl: &gl::Gl, color: [f32; 4]) {
    unsafe {
        gl.ClearColor(color[0], color[1], color[2], color[3]);
        gl.Clear(gl::COLOR_BUFFER_BIT);
        gl.DrawArrays(gl::TRIANGLES, 0, 3);
    }
}

#[rustfmt::skip]
static VERTEX_DATA: [f32; 15] = [
    -0.5, -0.5,  1.0,  0.0,  0.0,
     0.0,  0.5,  0.0,  1.0,  0.0,
     0.5, -0.5,  0.0,  0.0,  1.0,
];

const VS_SRC: &'static [u8] = b"
#version 100
precision mediump float;

attribute vec2 position;
attribute vec3 color;

varying vec3 v_color;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_color = color;
}
\0";

const FS_SRC: &'static [u8] = b"
#version 100
precision mediump float;

varying vec3 v_color;

void main() {
    gl_FragColor = vec4(v_color, 1.0);
}
\0";
