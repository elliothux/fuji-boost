//! M1 HAL demo: gradient, border and a square the d-pad moves. OK cycles the
//! square colour, BACK recentres it.
use crate::frame::App;
use crate::hal::gpio::*;
use crate::layout::{PRESENT_BASE, RENDER_HEIGHT, RENDER_WIDTH};

const W: i32 = RENDER_WIDTH as i32;
const H: i32 = RENDER_HEIGHT as i32;
const SIZE: i32 = 40;
const STEP: i32 = 6;
const BORDER: i32 = 4;
const COLORS: [u32; 4] = [0xffff_ffff, 0xffff_4040, 0xff40_ff40, 0xff40_40ff];

pub struct Hello {
    x: i32,
    y: i32,
    color: usize,
    previous: u32,
    drawn: Option<(i32, i32)>,
}

impl Hello {
    pub fn new() -> Self {
        Self {
            x: (W - SIZE) / 2,
            y: (H - SIZE) / 2,
            color: 0,
            previous: 0,
            drawn: None,
        }
    }
}

fn pixels() -> &'static mut [u32] {
    unsafe { core::slice::from_raw_parts_mut(PRESENT_BASE as *mut u32, (W * H) as usize) }
}

/// ARGB32 word, stored as BGRA bytes.
fn background(x: i32, y: i32) -> u32 {
    if x < BORDER || y < BORDER || x >= W - BORDER || y >= H - BORDER {
        return 0xffff_ffff;
    }
    let r = (x * 255 / (W - 1)) as u32;
    let g = (y * 255 / (H - 1)) as u32;
    0xff00_0080 | r << 16 | g << 8
}

fn fill(x0: i32, y0: i32, color: Option<u32>) {
    let buffer = pixels();
    for y in y0..y0 + SIZE {
        for x in x0..x0 + SIZE {
            buffer[(y * W + x) as usize] = color.unwrap_or_else(|| background(x, y));
        }
    }
}

impl App for Hello {
    fn frame(&mut self, buttons: u32, _: &[Option<crate::hal::touch::Point>; 2]) -> bool {
        let pressed = buttons & !self.previous;
        self.previous = buttons;
        let (x, y, color) = (self.x, self.y, self.color);
        if buttons & BTN_LEFT != 0 {
            self.x -= STEP;
        }
        if buttons & BTN_RIGHT != 0 {
            self.x += STEP;
        }
        if buttons & BTN_UP != 0 {
            self.y -= STEP;
        }
        if buttons & BTN_DOWN != 0 {
            self.y += STEP;
        }
        if pressed & BTN_CIRCLE != 0 {
            self.color = (self.color + 1) % COLORS.len();
        }
        if pressed & BTN_CROSS != 0 {
            self.x = (W - SIZE) / 2;
            self.y = (H - SIZE) / 2;
        }
        self.x = self.x.clamp(BORDER, W - BORDER - SIZE);
        self.y = self.y.clamp(BORDER, H - BORDER - SIZE);
        self.drawn.is_none() || (x, y, color) != (self.x, self.y, self.color)
    }

    fn present(&mut self) {
        match self.drawn {
            None => {
                let buffer = pixels();
                let mut red = [0u32; W as usize];
                for (x, value) in red.iter_mut().enumerate() {
                    *value = 0xff00_0080 | ((x as i32 * 255 / (W - 1)) as u32) << 16;
                }
                for y in 0..H {
                    let row = &mut buffer[(y * W) as usize..][..W as usize];
                    if y < BORDER || y >= H - BORDER {
                        row.fill(0xffff_ffff);
                        continue;
                    }
                    let green = ((y * 255 / (H - 1)) as u32) << 8;
                    for (pixel, &value) in row.iter_mut().zip(&red) {
                        *pixel = value | green;
                    }
                    row[..BORDER as usize].fill(0xffff_ffff);
                    row[(W - BORDER) as usize..].fill(0xffff_ffff);
                }
            }
            Some((x, y)) => fill(x, y, None),
        }
        fill(self.x, self.y, Some(COLORS[self.color]));
        self.drawn = Some((self.x, self.y));
    }
}
