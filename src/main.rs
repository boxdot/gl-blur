use glutin::{
    self,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, PossiblyCurrent,
};

use gl::types::*;

use std::mem;
use std::mem::MaybeUninit;
use std::ptr;

pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

fn main() {
    let el = EventLoop::new();
    let wb = WindowBuilder::new()
        .with_title("blur")
        .with_inner_size(glutin::dpi::LogicalSize::new(640.0, 480.0))
        .with_transparent(true);
    let windowed_context = ContextBuilder::new().build_windowed(wb, &el).unwrap();

    let windowed_context = unsafe { windowed_context.make_current().unwrap() };

    let context = Context::load(&windowed_context);

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
                unsafe { context.draw_frame([0.13, 0.13, 0.13, 0.3]) };
                windowed_context.swap_buffers().unwrap();
            }
            _ => (),
        }
    });
}

struct Context {
    gl: gl::Gl,
    /// scene rendering
    program: GLuint,
    /// blur rendering
    blur_program: GLuint,
    /// texture for the rendered scene and the first blur pass
    tex: [GLuint; 2],
    /// framebuffers for the rendered scene and the first blur pass
    fb: [GLuint; 2],
    /// scene objects
    scene_vao: GLuint,
    /// screen (blur) objects
    screen_vao: GLuint,
    /// texture blur shader parameter
    texture_uniform: GLint,
    /// texture size blur shader parameter
    texture_size_uniform: GLint,
    /// Gauss blur shader parameter
    sigma_uniform: GLint,
    /// blur direction parameter
    dir_uniform: GLint,
}

impl Context {
    fn load(gl_context: &glutin::Context<PossiblyCurrent>) -> Self {
        let gl = gl::Gl::load_with(|ptr| gl_context.get_proc_address(ptr) as *const _);

        let version = unsafe {
            let data = std::ffi::CStr::from_ptr(gl.GetString(gl::VERSION) as *const _)
                .to_bytes()
                .to_vec();
            String::from_utf8(data).unwrap()
        };
        println!("OpenGL version {}", version);

        let vs = create_shader(&gl, gl::VERTEX_SHADER, VS_SRC).unwrap();
        let fs = create_shader(&gl, gl::FRAGMENT_SHADER, FS_SRC).unwrap();
        let blur_vs = create_shader(&gl, gl::VERTEX_SHADER, BLUR_VS_SRC).unwrap();
        let blur_fs = create_shader(&gl, gl::FRAGMENT_SHADER, BLUR_FS_SRC).unwrap();

        unsafe {
            let program = gl.CreateProgram();
            gl.AttachShader(program, vs);
            gl.AttachShader(program, fs);
            gl.LinkProgram(program);

            let pos_attrib = gl.GetAttribLocation(program, b"position\0".as_ptr() as *const _);
            let color_attrib = gl.GetAttribLocation(program, b"color\0".as_ptr() as *const _);

            let mut tri_buf = MaybeUninit::zeroed();
            gl.GenBuffers(1, tri_buf.as_mut_ptr());
            let tri_buf = tri_buf.assume_init();
            gl.BindBuffer(gl::ARRAY_BUFFER, tri_buf);
            gl.BufferData(
                gl::ARRAY_BUFFER,
                (VERTEX_DATA.len() * mem::size_of::<f32>()) as GLsizeiptr,
                VERTEX_DATA.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            let scene_vao = {
                let mut vao = MaybeUninit::zeroed();
                gl.GenVertexArrays(1, vao.as_mut_ptr());
                let vao = vao.assume_init();
                gl.BindVertexArray(vao);

                gl.VertexAttribPointer(
                    pos_attrib as GLuint,
                    2,                                    // size
                    gl::FLOAT,                            // type
                    gl::FALSE,                            // normalized
                    5 * mem::size_of::<f32>() as GLsizei, // stride
                    ptr::null(),                          // pointer
                );
                gl.VertexAttribPointer(
                    color_attrib as GLuint,
                    3,
                    gl::FLOAT,
                    gl::FALSE,
                    5 * mem::size_of::<f32>() as GLsizei,
                    (2 * mem::size_of::<f32>()) as *const _,
                );
                gl.EnableVertexAttribArray(pos_attrib as GLuint);
                gl.EnableVertexAttribArray(color_attrib as GLuint);

                gl.BindVertexArray(0);
                vao
            };

            let blur_program = gl.CreateProgram();
            gl.AttachShader(blur_program, blur_vs);
            gl.AttachShader(blur_program, blur_fs);
            gl.LinkProgram(blur_program);

            let position_attrib =
                gl.GetAttribLocation(blur_program, b"position\0".as_ptr() as *const _);
            let texture_uniform =
                gl.GetUniformLocation(blur_program, b"u_texture\0".as_ptr() as *const _);
            let texture_size_uniform =
                gl.GetUniformLocation(blur_program, b"u_textureSize\0".as_ptr() as *const _);
            let sigma_uniform =
                gl.GetUniformLocation(blur_program, b"u_sigma\0".as_ptr() as *const _);
            let dir_uniform = gl.GetUniformLocation(blur_program, b"u_dir\0".as_ptr() as *const _);

            let mut quad_buf = MaybeUninit::zeroed();
            gl.GenBuffers(1, quad_buf.as_mut_ptr());
            let quad_buf = quad_buf.assume_init();
            gl.BindBuffer(gl::ARRAY_BUFFER, quad_buf);
            gl.BufferData(
                gl::ARRAY_BUFFER,
                (SCREEN_RECT2.len() * mem::size_of::<f32>()) as GLsizeiptr,
                SCREEN_RECT2.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            // screen vao
            let screen_vao = {
                let mut vao = MaybeUninit::zeroed();
                gl.GenVertexArrays(1, vao.as_mut_ptr());
                let vao = vao.assume_init();
                gl.BindVertexArray(vao);

                gl.EnableVertexAttribArray(position_attrib as GLuint);
                gl.VertexAttribPointer(
                    position_attrib as GLuint,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
                gl.BindVertexArray(0);
                vao
            };

            // Note: viewport size is not necessarily the same as window size
            let (width, height) = get_viewport_size(&gl);

            // generate framebuffers for rendering into texture
            let mut fb = [MaybeUninit::<GLuint>::zeroed(); 2];
            let mut tex = [MaybeUninit::<GLuint>::zeroed(); 2];

            gl.GenFramebuffers(2, fb.as_mut_ptr() as *mut _);
            gl.GenTextures(2, tex.as_mut_ptr() as *mut _);

            let fb = mem::transmute::<_, [GLuint; 2]>(fb);
            let tex = mem::transmute::<_, [GLuint; 2]>(tex);

            for i in 0..2 {
                gl.BindFramebuffer(gl::FRAMEBUFFER, fb[i]);
                gl.BindTexture(gl::TEXTURE_2D, tex[i]);
                gl.TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA as GLint,
                    width as GLint,
                    height as GLint,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    ptr::null(),
                );
                gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
                gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
                gl.BindTexture(gl::TEXTURE_2D, 0);

                gl.FramebufferTexture2D(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    gl::TEXTURE_2D,
                    tex[i],
                    0,
                );

                let mut renderbuffer = MaybeUninit::zeroed();
                gl.GenFramebuffers(1, renderbuffer.as_mut_ptr());
                let renderbuffer = renderbuffer.assume_init();
                gl.BindRenderbuffer(gl::RENDERBUFFER, renderbuffer);
                gl.RenderbufferStorage(
                    gl::RENDERBUFFER,
                    gl::DEPTH,
                    width as GLint,
                    height as GLint,
                );
                gl.BindRenderbuffer(gl::RENDERBUFFER, 0);
                gl.FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    gl::DEPTH_ATTACHMENT,
                    gl::RENDERBUFFER,
                    renderbuffer,
                );

                if gl.CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                    panic!("Framebuffer is not complete!")
                }
            }
            gl.BindFramebuffer(gl::FRAMEBUFFER, 0);

            Context {
                gl,
                program,
                blur_program,
                tex,
                fb,
                scene_vao,
                screen_vao,
                texture_uniform,
                texture_size_uniform,
                sigma_uniform,
                dir_uniform,
            }
        }
    }

    unsafe fn draw_frame(&self, color: [f32; 4]) {
        let gl = &self.gl;
        // draw scene
        gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb[0]);
        gl.ClearColor(color[0], color[1], color[2], color[3]);
        gl.Clear(gl::COLOR_BUFFER_BIT);

        gl.UseProgram(self.program);
        gl.BindVertexArray(self.scene_vao);
        gl.DrawArrays(gl::TRIANGLES, 0, 3);

        // pass 1
        gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb[1]);
        gl.ClearColor(1.0, 1.0, 1.0, 1.0);
        gl.Clear(gl::COLOR_BUFFER_BIT);

        gl.UseProgram(self.blur_program);
        gl.BindVertexArray(self.screen_vao);
        gl.BindTexture(gl::TEXTURE_2D, self.tex[0]);

        let (width, height) = get_viewport_size(gl);
        gl.Uniform1i(self.texture_uniform, 0);
        gl.Uniform2f(self.texture_size_uniform, width, height);
        gl.Uniform1f(self.sigma_uniform, 0.5);
        gl.Uniform2f(self.dir_uniform, 1.0, 0.0);

        gl.ClearColor(color[0], color[1], color[2], color[3]);
        gl.Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        gl.DrawArrays(gl::TRIANGLE_STRIP, 0, 4);

        // pass 2

        gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl.ClearColor(1.0, 1.0, 1.0, 1.0);
        gl.Clear(gl::COLOR_BUFFER_BIT);

        gl.BindTexture(gl::TEXTURE_2D, self.tex[1]);
        gl.Uniform2f(self.dir_uniform, 0.0, 1.0);

        gl.DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
    }
}

fn create_shader(gl: &gl::Gl, kind: GLuint, source: &[u8]) -> Result<GLuint, String> {
    unsafe {
        let id = gl.CreateShader(kind);
        gl.ShaderSource(id, 1, [source.as_ptr() as *const _].as_ptr(), ptr::null());
        gl.CompileShader(id);

        let mut success = 1;
        gl.GetShaderiv(id, gl::COMPILE_STATUS, &mut success);
        if success == 0 {
            let mut len = 0;
            gl.GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut len);

            let mut buffer: Vec<u8> = Vec::with_capacity(len as usize);
            gl.GetShaderInfoLog(id, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut _);
            buffer.set_len(len as usize);
            Err(String::from_utf8_unchecked(buffer))
        } else {
            Ok(id)
        }
    }
}

unsafe fn get_viewport_size(gl: &gl::Gl) -> (f32, f32) {
    let mut viewport = [0f32; 4];
    gl.GetFloatv(gl::VIEWPORT, viewport.as_mut_ptr());
    (viewport[2], viewport[3])
}

/// Triangle 2D vertices and colors
#[rustfmt::skip]
static VERTEX_DATA: [f32; 15] = [
    // vertex    // color
    -0.5, -0.5,  1.0,  0.0,  0.0,
     0.0,  0.5,  0.0,  1.0,  0.0,
     0.5, -0.5,  0.0,  0.0,  1.0,
];

/// 2D screen rectange
#[rustfmt::skip]
const SCREEN_RECT2: [f32; 8] = [
    -1.0, -1.0,
    1.0, -1.0,
    -1.0, 1.0,
    1.0, 1.0,
];

/// Scene vector shader
const VS_SRC: &[u8] = b"
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

/// Scene fragment shader
const FS_SRC: &[u8] = b"
#version 100
precision mediump float;

varying vec3 v_color;

void main() {
    gl_FragColor = vec4(v_color, 1.0);
}
\0";

/// Blur vector shader
///
/// source: https://stackoverflow.com/questions/44779142/opengl-es-2-0-gaussian-blur-on-triangle
const BLUR_VS_SRC: &[u8] = b"
#version 100
precision mediump float;

attribute vec2 position;
varying   vec2 v_position;

void main() {
    v_position = position;
    gl_Position = vec4( position, 0.0, 1.0 );
}
\0";

/// Blur fragment shader
///
/// source: https://stackoverflow.com/questions/44779142/opengl-es-2-0-gaussian-blur-on-triangle
const BLUR_FS_SRC: &[u8] = b"
#version 100
precision mediump float;
varying vec2 v_position;

uniform sampler2D u_texture;
uniform vec2      u_textureSize;
uniform float     u_sigma;
uniform vec2      u_dir;

float CalcGauss( float x, float sigma )
{
    if ( sigma <= 0.0 )
        return 0.0;
    return exp( -(x*x) / (2.0 * sigma) ) / (2.0 * 3.14157 * sigma);
}

void main()
{
    vec2 texC     = v_position.st * 0.5 + 0.5;
    vec4 texCol   = texture2D( u_texture, texC );
    vec4 gaussCol = vec4( texCol.rgb, 1.0 );
    vec2 step     = u_dir / u_textureSize;
    for ( int i = 1; i <= 32; ++ i )
    {
        float weight = CalcGauss( float(i) / 32.0, u_sigma * 0.5 );
        if ( weight < 1.0/255.0 )
            break;
        texCol    = texture2D( u_texture, texC + step * float(i) );
        gaussCol += vec4( texCol.rgb * weight, weight );
        texCol    = texture2D( u_texture, texC - step * float(i) );
        gaussCol += vec4( texCol.rgb * weight, weight );
    }
    gaussCol.rgb = clamp( gaussCol.rgb / gaussCol.w, 0.0, 1.0 );
    gl_FragColor = vec4( gaussCol.rgb, 1.0 );
}
\0";
