//! WebGL2 3D Waterfall renderer.

use wasm_bindgen::prelude::*;
use web_sys::{WebGl2RenderingContext as GL, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture, WebGlVertexArrayObject};

#[derive(Clone)]
pub struct Waterfall3D {
    gl: GL,
    program: WebGlProgram,
    vao: WebGlVertexArrayObject,
    vbo: WebGlBuffer,
    ibo: WebGlBuffer,
    tex: WebGlTexture,
    idx_count: i32,
    tex_w: i32,
    tex_h: i32,
}

fn compile(gl: &GL, ty: u32, src: &str) -> Result<WebGlShader, JsValue> {
    let sh = gl.create_shader(ty).ok_or("create_shader failed")?;
    gl.shader_source(&sh, src);
    gl.compile_shader(&sh);
    if gl.get_shader_parameter(&sh, GL::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(sh)
    } else {
        Err(JsValue::from_str(&gl.get_shader_info_log(&sh).unwrap_or_else(|| "shader compile error".into())))
    }
}

fn link(gl: &GL, vs: &WebGlShader, fs: &WebGlShader) -> Result<WebGlProgram, JsValue> {
    let p = gl.create_program().ok_or("create_program failed")?;
    gl.attach_shader(&p, vs);
    gl.attach_shader(&p, fs);
    gl.link_program(&p);
    if gl.get_program_parameter(&p, GL::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(p)
    } else {
        Err(JsValue::from_str(&gl.get_program_info_log(&p).unwrap_or_else(|| "program link error".into())))
    }
}

const VERT: &str = r#"#version 300 es
precision highp float;
precision highp sampler2D;

layout(location=0) in vec2 a_pos;  // grid coords in [0,1] time, [0,1] freq
uniform sampler2D u_tex;           // R32F texture: [tBins x fBins]
uniform ivec2 u_texSize;
uniform float u_zgain;
uniform float u_floorDb;
uniform float u_contrast;
uniform mat4 u_viewProj;
uniform float u_texOffset;
uniform float u_texScale;

out float v_db;
out vec2 v_uv;

float normDb(float db){
  float x = clamp((db - u_floorDb) / (0.0 - u_floorDb), 0.0, 1.0);
  float p = 1.0 / max(0.01, u_contrast);
  return pow(x, p);
}

void main(){
  // Calculate texture coordinate
  float tx_float = (u_texOffset + a_pos.x * u_texScale);
  int tx = int(floor(tx_float * float(u_texSize.x - 1)));
  int ty = int(floor(a_pos.y * float(u_texSize.y - 1)));

  float db = texelFetch(u_tex, ivec2(tx, ty), 0).r;
  float h = normDb(db);

  float x = (a_pos.x - 0.5) * 3.0;
  float y = (a_pos.y - 0.5) * 2.0;
  float z = h * u_zgain;

  v_db = db;
  v_uv = vec2(tx_float, a_pos.y);
  gl_Position = u_viewProj * vec4(x, y, z, 1.0);
}
"#;

const FRAG: &str = r#"#version 300 es
precision highp float;
precision highp sampler2D;

in float v_db;
in vec2 v_uv;
out vec4 o;

uniform float u_floorDb;
uniform float u_contrast;
uniform sampler2D u_tex;
uniform ivec2 u_texSize;
uniform float u_showAccel;

float normDb(float db){
  float x = clamp((db - u_floorDb) / (0.0 - u_floorDb), 0.0, 1.0);
  float p = 1.0 / max(0.01, u_contrast);
  return pow(x, p);
}

vec3 palette(float t){
  return vec3(
    smoothstep(0.0,1.0,t),
    smoothstep(0.15,0.95,t*t),
    smoothstep(0.05,1.0,sqrt(t))
  );
}

void main(){
  float h = normDb(v_db);
  vec3 col = palette(h);

  if (u_showAccel > 0.5) {
      // Simple heuristic for pitch slope
      float dx = 1.0 / float(u_texSize.x);
      float dy = 1.0 / float(u_texSize.y);

      float val = h;

      // Only compute if signal is strong enough
      if (val > 0.2) {
          // We look slightly back in time (left) to see where the energy came from
          float v_left = normDb(texture(u_tex, v_uv - vec2(dx, 0.0)).r);
          float v_left_down = normDb(texture(u_tex, v_uv - vec2(dx, dy)).r); // came from lower freq
          float v_left_up = normDb(texture(u_tex, v_uv - vec2(dx, -dy)).r);  // came from higher freq

          float score_up = v_left_down;     // rising pitch
          float score_down = v_left_up;     // falling pitch

          if (score_up > score_down && score_up > v_left) {
             col = mix(col, vec3(1.0, 0.2, 0.2), score_up * 0.8); // Red for rising
          } else if (score_down > score_up && score_down > v_left) {
             col = mix(col, vec3(0.2, 0.2, 1.0), score_down * 0.8); // Blue for falling
          }
      }
  }

  // Grid lines (scrolls with data if we use v_uv.x, but we might want fixed grid?)
  // If we use v_uv.x, the grid moves with the waterfall.
  // If we use gl_FragCoord or a passed 'uv_screen', it stays fixed.
  // Waterfall usually has moving grid or fixed time ruler.
  // Let's keep it moving with data for now.
  float gx = abs(fract(v_uv.x*100.0) - 0.5);
  float gy = abs(fract(v_uv.y*60.0) - 0.5);
  float grid = smoothstep(0.48,0.5, max(gx,gy));
  col = mix(col, col*0.65, grid*0.35);
  o = vec4(col, 1.0);
}
"#;

impl Waterfall3D {
    pub fn new(gl: GL) -> Result<Self, JsValue> {
        let vs = compile(&gl, GL::VERTEX_SHADER, VERT)?;
        let fs = compile(&gl, GL::FRAGMENT_SHADER, FRAG)?;
        let program = link(&gl, &vs, &fs)?;

        let vao = gl.create_vertex_array().ok_or("create_vertex_array failed")?;
        gl.bind_vertex_array(Some(&vao));

        let vbo = gl.create_buffer().ok_or("create_buffer failed")?;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
        // placeholder quad
        let verts: [f32; 8] = [0.0,0.0, 1.0,0.0, 0.0,1.0, 1.0,1.0];
        unsafe {
            let arr = js_sys::Float32Array::view(&verts);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 2, GL::FLOAT, false, 0, 0);

        let ibo = gl.create_buffer().ok_or("create_buffer failed")?;
        gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&ibo));
        let indices: [u32; 6] = [0,1,2, 2,1,3];
        unsafe {
            let arr = js_sys::Uint32Array::view(&indices);
            gl.buffer_data_with_array_buffer_view(GL::ELEMENT_ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }

        let tex = gl.create_texture().ok_or("create_texture failed")?;
        gl.bind_texture(GL::TEXTURE_2D, Some(&tex));
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);

        // 1x1 init
        let one: [f32; 1] = [-120.0];
        unsafe {
            let arr = js_sys::Float32Array::view(&one);
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_array_buffer_view(
                GL::TEXTURE_2D, 0, GL::R32F as i32, 1, 1, 0, GL::RED, GL::FLOAT, Some(&arr)
            )?;
        }

        gl.bind_vertex_array(None);

        Ok(Self {
            gl,
            program,
            vao,
            vbo,
            ibo,
            tex,
            idx_count: 6,
            tex_w: 1,
            tex_h: 1,
        })
    }

    /// Upload a spectrogram (dB values) to the GPU.
    /// `data_db` must be length `t_bins * f_bins`, row-major time-major (x changes fastest).
    pub fn upload_db_texture(&mut self, t_bins: i32, f_bins: i32, data_db: &[f32]) -> Result<(), JsValue> {
        self.tex_w = t_bins.max(1);
        self.tex_h = f_bins.max(1);

        self.gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
        unsafe {
            let arr = js_sys::Float32Array::view(data_db);
            self.gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_array_buffer_view(
                GL::TEXTURE_2D, 0, GL::R32F as i32, self.tex_w, self.tex_h, 0, GL::RED, GL::FLOAT, Some(&arr)
            )?;
        }

        // rebuild mesh grid + indices for (t_bins x f_bins)
        // Note: For animated/scrolling view, we might not need to rebuild the grid to match t_bins perfectly
        // if we are just mapping UVs. But a dense grid allows vertex displacement (z-height) to work well.
        // If we only show a slice, we still need enough vertices in the view frustum.
        // Ideally the grid resolution should be decoupled from texture resolution, but here it is coupled.
        // If the texture is huge, this is expensive.
        // For now, I'll keep it as is, but this might be slow for long files.
        // Optimization: Fix grid size (e.g. 500x200) and sample texture in vertex shader.
        // But the current implementation rebuilds grid to match texture size.
        self.rebuild_grid(self.tex_w, self.tex_h)?;

        Ok(())
    }

    fn rebuild_grid(&mut self, t_bins: i32, f_bins: i32) -> Result<(), JsValue> {
        // Limit grid size to avoid too many vertices?
        // If t_bins is 10000, we have 20k triangles * f_bins... too much.
        // Let's cap the grid resolution and rely on texture sampling.
        // But if we cap grid, 'a_pos' won't match texels perfectly.
        // However, we are now using u_texOffset/Scale, so we are scrolling the texture over the mesh.
        // So the mesh should just have "enough" resolution.

        // Let's cap the mesh resolution to something reasonable like 500x256
        let mesh_t = t_bins.min(512).max(2) as usize;
        let mesh_f = f_bins.min(256).max(2) as usize;

        // vertices: (mesh_t*mesh_f) of vec2 (u,v)
        let mut verts: Vec<f32> = Vec::with_capacity(mesh_t * mesh_f * 2);
        for y in 0..mesh_f {
            let v = if mesh_f == 1 { 0.0 } else { y as f32 / (mesh_f as f32 - 1.0) };
            for x in 0..mesh_t {
                let u = if mesh_t == 1 { 0.0 } else { x as f32 / (mesh_t as f32 - 1.0) };
                verts.push(u);
                verts.push(v);
            }
        }

        let cells_x = mesh_t - 1;
        let cells_y = mesh_f - 1;
        let tri_count = cells_x * cells_y * 2;
        let mut idx: Vec<u32> = Vec::with_capacity(tri_count * 3);
        for y in 0..cells_y {
            for x in 0..cells_x {
                let a = (y * mesh_t + x) as u32;
                let b = a + 1;
                let c = a + mesh_t as u32;
                let d = c + 1;
                // Use Counter-Clockwise (CCW) winding order for standard GL front face
                // Triangle 1: a -> b -> c
                // Triangle 2: b -> d -> c
                idx.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }

        self.idx_count = idx.len() as i32;

        self.gl.bind_vertex_array(Some(&self.vao));

        self.gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.vbo));
        unsafe {
            let arr = js_sys::Float32Array::view(&verts);
            self.gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }

        self.gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
        unsafe {
            let arr = js_sys::Uint32Array::view(&idx);
            self.gl.buffer_data_with_array_buffer_view(GL::ELEMENT_ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }

        self.gl.bind_vertex_array(None);
        Ok(())
    }

    /// Draw one frame.
    /// You supply a 4x4 view-projection matrix (column-major f32[16]).
    pub fn draw(&self, viewport_w: i32, viewport_h: i32, view_proj: &[f32;16], zgain: f32, floor_db: f32, contrast: f32, tex_offset: f32, tex_scale: f32, show_accel: bool) {
        let gl = &self.gl;
        gl.viewport(0, 0, viewport_w, viewport_h);
        gl.enable(GL::DEPTH_TEST);

        // Disable backface culling to ensure mesh visibility regardless of camera orientation/winding
        gl.disable(GL::CULL_FACE);

        gl.clear_color(0.04, 0.05, 0.08, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));

        // uniforms
        let loc_vp = gl.get_uniform_location(&self.program, "u_viewProj");
        gl.uniform_matrix4fv_with_f32_array(loc_vp.as_ref(), false, view_proj);

        let loc_ts = gl.get_uniform_location(&self.program, "u_texSize");
        gl.uniform2i(loc_ts.as_ref(), self.tex_w, self.tex_h);

        let loc_z = gl.get_uniform_location(&self.program, "u_zgain");
        gl.uniform1f(loc_z.as_ref(), zgain);

        let loc_f = gl.get_uniform_location(&self.program, "u_floorDb");
        gl.uniform1f(loc_f.as_ref(), floor_db);

        let loc_c = gl.get_uniform_location(&self.program, "u_contrast");
        gl.uniform1f(loc_c.as_ref(), contrast);

        let loc_to = gl.get_uniform_location(&self.program, "u_texOffset");
        gl.uniform1f(loc_to.as_ref(), tex_offset);

        let loc_tsc = gl.get_uniform_location(&self.program, "u_texScale");
        gl.uniform1f(loc_tsc.as_ref(), tex_scale);

        let loc_sa = gl.get_uniform_location(&self.program, "u_showAccel");
        gl.uniform1f(loc_sa.as_ref(), if show_accel { 1.0 } else { 0.0 });

        // texture
        gl.active_texture(GL::TEXTURE0);
        gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
        let loc_tex = gl.get_uniform_location(&self.program, "u_tex");
        gl.uniform1i(loc_tex.as_ref(), 0);

        gl.draw_elements_with_i32(GL::TRIANGLES, self.idx_count, GL::UNSIGNED_INT, 0);

        gl.bind_vertex_array(None);
        gl.use_program(None);
    }
}
