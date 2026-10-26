//! Precompiled SVG triangle geometry, uploaded once without image textures.
use gl::types::GLuint;
use std::{fs, path::Path};

pub struct Mesh {
    pub buffer: GLuint,
    pub width: f32,
    pub height: f32,
    pub vertices: i32,
    pub bytes: usize,
}

impl Mesh {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            fs::read(path).map_err(|e| format!("Vector artwork {}: {e}", path.display()))?;
        if bytes.len() < 20 || &bytes[..8] != b"LUMAVEC1" {
            return Err(format!("Invalid vector artwork header: {}", path.display()));
        }
        let float = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let width = float(8);
        let height = float(12);
        let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || count == 0
            || count > 1_000_000
            || !count.is_multiple_of(3)
            || bytes.len() != 20 + count * 36
        {
            return Err(format!(
                "Invalid vector artwork dimensions: {}",
                path.display()
            ));
        }
        let vertices: Vec<f32> = bytes[20..]
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        if vertices.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "Invalid vector artwork coordinates: {}",
                path.display()
            ));
        }
        Self::upload(width, height, &vertices)
    }

    pub fn upload(width: f32, height: f32, vertices: &[f32]) -> Result<Self, String> {
        let count = vertices.len() / 9;
        if count == 0 || !count.is_multiple_of(3) || vertices.len() != count * 9 {
            return Err("Invalid vector buffer vertex count".into());
        }
        if vertices.iter().any(|value| !value.is_finite()) {
            return Err("Invalid vector buffer coordinates".into());
        }
        let mut buffer = 0;
        unsafe {
            gl::GenBuffers(1, &mut buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                std::mem::size_of_val(vertices) as isize,
                vertices.as_ptr().cast(),
                gl::STATIC_DRAW,
            );
        }
        Ok(Self {
            buffer,
            width,
            height,
            vertices: count as i32,
            bytes: count * 36,
        })
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        unsafe { gl::DeleteBuffers(1, &self.buffer) };
    }
}
