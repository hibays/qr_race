//! Port of `zxing/common/bytematrix.{hpp,cpp}`: row-contiguous grayscale storage.

pub struct ByteMatrix {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

impl ByteMatrix {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            bytes: vec![0u8; width * height],
        }
    }

    pub fn from_vec(width: usize, height: usize, bytes: Vec<u8>) -> Self {
        debug_assert_eq!(bytes.len(), width * height);
        Self {
            width,
            height,
            bytes,
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.bytes[y * self.width + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: u8) {
        let w = self.width;
        self.bytes[y * w + x] = v;
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn row(&self, y: usize) -> &[u8] {
        &self.bytes[y * self.width..(y + 1) * self.width]
    }

    #[inline]
    pub fn row_mut(&mut self, y: usize) -> &mut [u8] {
        let w = self.width;
        &mut self.bytes[y * w..(y + 1) * w]
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}
