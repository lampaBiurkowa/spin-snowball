use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::{GenericImageView, Pixel};
use ndarray::Array2;
use serde::Serialize;

// ------------------------------------------------------------
// Helpers
// ------------------------------------------------------------

#[derive(Clone, Copy)]
struct RGBA {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

fn load_rgba(path: &Path) -> (Vec<RGBA>, u32, u32) {
    let img = image::open(path).expect("Failed to open image").to_rgba8();

    let (width, height) = img.dimensions();

    let data = img
        .pixels()
        .map(|p| {
            let c = p.channels();
            RGBA {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            }
        })
        .collect();

    (data, width, height)
}

fn alpha_mask(data: &[RGBA], width: u32, height: u32) -> Array2<bool> {
    let mut mask = Array2::<bool>::default((height as usize, width as usize));
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            mask[(y as usize, x as usize)] = data[idx].a > 0;
        }
    }
    mask
}
fn bit_mask(c: u8) -> bool {
    (c & 1) == 1
}

fn decode_rgb(r: u8, g: u8, b: u8, a: u8) -> (bool, Vec<&'static str>) {
    let mut mask = Vec::new();

    if bit_mask(r) {
        mask.push("snowball");
    }
    if bit_mask(g) {
        mask.push("ball");
    }
    if bit_mask(b) {
        mask.push("player_team1");
    }
    if bit_mask(a) {
        mask.push("player_team2");
    }
    let is_hole = r == 127 && b == 127 && g == 127;

    (is_hole, mask)
}

fn strip_mask_bit(c: u8) -> f32 {
    let base = c & 0b1111_1110; // clear LSB
    base as f32 / 255.0
}

// ------------------------------------------------------------
// Connected Components (4-connected, same behavior as scipy.ndimage.label)
// ------------------------------------------------------------

fn label_components(mask: &Array2<bool>) -> (Array2<i32>, i32) {
    let (h, w) = mask.dim();
    let mut labels = Array2::<i32>::zeros((h, w));
    let mut current_label = 0;

    fn flood_fill(
        mask: &Array2<bool>,
        labels: &mut Array2<i32>,
        start_x: isize,
        start_y: isize,
        label: i32,
    ) {
        let h = mask.dim().0 as isize;
        let w = mask.dim().1 as isize;

        let mut stack = Vec::new();
        stack.push((start_x, start_y));

        while let Some((x, y)) = stack.pop() {
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let (ux, uy) = (x as usize, y as usize);

            if !mask[(uy, ux)] {
                continue;
            }
            if labels[(uy, ux)] != 0 {
                continue;
            }

            labels[(uy, ux)] = label;

            stack.push((x + 1, y));
            stack.push((x - 1, y));
            stack.push((x, y + 1));
            stack.push((x, y - 1));
        }
    }


    for y in 0..h {
        for x in 0..w {
            if mask[(y, x)] && labels[(y, x)] == 0 {
                current_label += 1;
                flood_fill(mask, &mut labels, x as isize, y as isize, current_label);
            }
        }
    }

    (labels, current_label)
}

// ------------------------------------------------------------
// Data Structures
// ------------------------------------------------------------

#[derive(Serialize)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[derive(Serialize)]
struct RectData {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    is_hole: bool,
    factor: f32,
    color: Color,
    mask: Vec<String>,
}

#[derive(Serialize)]
struct RectObject {
    rect: RectData,
}

#[derive(Serialize)]
struct CircleData {
    x: f32,
    y: f32,
    radius: f32,
    is_hole: bool,
    factor: f32,
    color: Color,
    mask: Vec<String>,
}

#[derive(Serialize)]
struct CircleObject {
    circle: CircleData,
}

#[derive(Serialize)]
struct Goal {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    team: String,
}

// ------------------------------------------------------------
// Extraction logic
// ------------------------------------------------------------

fn extract_rectangles(data: &[RGBA], width: u32, height: u32) -> Vec<RectObject> {
    let mask = alpha_mask(data, width, height);
    let (labels, count) = label_components(&mask);

    let mut objects = Vec::new();

    for label in 1..=count {
        let mut xs = Vec::new();
        let mut ys = Vec::new();

        for y in 0..height as usize {
            for x in 0..width as usize {
                if labels[(y, x)] == label {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }

        let x0 = *xs.iter().min().unwrap();
        let x1 = *xs.iter().max().unwrap();
        let y0 = *ys.iter().min().unwrap();
        let y1 = *ys.iter().max().unwrap();

        let idx = y0 * width as usize + x0;
        let px = data[idx];

        let (is_hole, mask) = decode_rgb(px.r, px.g, px.b, px.a);

        objects.push(RectObject {
            rect: RectData {
                x: x0 as i32,
                y: y0 as i32,
                w: (x1 - x0 + 1) as i32,
                h: (y1 - y0 + 1) as i32,
                is_hole,
                factor: 1.0,
                color: Color {
                    r: strip_mask_bit(px.r),
                    g: strip_mask_bit(px.g),
                    b: strip_mask_bit(px.b),
                    a: 1.0,
                },
                mask: if is_hole {
                    vec![]
                } else {
                    mask.into_iter().map(String::from).collect()
                },
            },
        });
    }

    objects
}

fn extract_circles(data: &[RGBA], width: u32, height: u32) -> Vec<CircleObject> {
    let mask = alpha_mask(data, width, height);
    let (labels, count) = label_components(&mask);

    let mut objects = Vec::new();

    for label in 1..=count {
        let mut xs = Vec::new();
        let mut ys = Vec::new();

        for y in 0..height as usize {
            for x in 0..width as usize {
                if labels[(y, x)] == label {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }

        let x0 = *xs.iter().min().unwrap();
        let x1 = *xs.iter().max().unwrap();
        let y0 = *ys.iter().min().unwrap();
        let y1 = *ys.iter().max().unwrap();

        let cx = (x0 + x1) as f32 / 2.0;
        let cy = (y0 + y1) as f32 / 2.0;
        let radius = ((x1 - x0).max(y1 - y0)) as f32 / 2.0;

        let sample_x = xs[0];
        let sample_y = ys[0];
        let idx = sample_y * width as usize + sample_x;
        let px = data[idx];

        let (is_hole, mask) = decode_rgb(px.r, px.g, px.b, px.a);

        objects.push(CircleObject {
            circle: CircleData {
                x: cx,
                y: cy,
                radius,
                is_hole,
                factor: 1.0,
                color: Color {
                    r: strip_mask_bit(px.r),
                    g: strip_mask_bit(px.g),
                    b: strip_mask_bit(px.b),
                    a: 1.0,
                },
                mask: if is_hole {
                    vec![]
                } else {
                    mask.into_iter().map(String::from).collect()
                },
            },
        });
    }

    objects
}

fn extract_goals(data: &[RGBA], width: u32, height: u32) -> Vec<Goal> {
    let mask = alpha_mask(data, width, height);
    let (labels, count) = label_components(&mask);

    let mut goals = Vec::new();

    for label in 1..=count {
        let mut xs = Vec::new();
        let mut ys = Vec::new();

        for y in 0..height as usize {
            for x in 0..width as usize {
                if labels[(y, x)] == label {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }

        let x0 = *xs.iter().min().unwrap();
        let x1 = *xs.iter().max().unwrap();
        let y0 = *ys.iter().min().unwrap();
        let y1 = *ys.iter().max().unwrap();

        let idx = y0 * width as usize + x0;
        let px = data[idx];

        let team = if px.r == 255 { "Team1" } else { "Team2" };

        goals.push(Goal {
            x: x0 as i32,
            y: y0 as i32,
            w: (x1 - x0 + 1) as i32,
            h: (y1 - y0 + 1) as i32,
            team: team.to_string(),
        });
    }

    goals
}

// ------------------------------------------------------------
// Main
// ------------------------------------------------------------

#[derive(Serialize)]
struct MapData {
    name: String,
    width: u32,
    height: u32,
    physics: serde_json::Value,
    team1: serde_json::Value,
    team2: serde_json::Value,
    ball: serde_json::Value,
    goals: Vec<Goal>,
    objects: Vec<serde_json::Value>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: png_map_to_json <map_directory>");
        std::process::exit(1);
    }

    let map_dir = PathBuf::from(&args[1]);

    let (rects, w, h) = load_rgba(&map_dir.join("rects.png"));
    let (circles, _, _) = load_rgba(&map_dir.join("circles.png"));
    let (goals_img, _, _) = load_rgba(&map_dir.join("goals.png"));

    let mut objects = Vec::new();

    for r in extract_rectangles(&rects, w, h) {
        objects.push(serde_json::to_value(r).unwrap());
    }
    for c in extract_circles(&circles, w, h) {
        objects.push(serde_json::to_value(c).unwrap());
    }

    let data = MapData {
        name: map_dir.file_name().unwrap().to_string_lossy().to_string(),
        width: w,
        height: h,
        physics: serde_json::json!({
            "player_radius": 25.0,
            "player_mass": 1.0,
            "snowball_radius": 8.0,
            "snowball_mass": 0.5,
            "ball_mass": 1.0,
            "ball_radius": 10.0,
            "ball_bounciness": 0.7,
            "player_bounciness": 0.6,
            "snowball_bounciness": 0.9,
            "snowball_lifetime_sec": 3.0,
            "friction_per_frame": 0.99
        }),
        team1: serde_json::json!({ "spawn_x": w as f32 * 0.25, "spawn_y": h as f32 * 0.5 }),
        team2: serde_json::json!({ "spawn_x": w as f32 * 0.75, "spawn_y": h as f32 * 0.5 }),
        ball: serde_json::json!({ "spawn_x": w as f32 * 0.5,  "spawn_y": h as f32 * 0.5 }),
        goals: extract_goals(&goals_img, w, h),
        objects,
    };

    let out_path = map_dir.join("map.json");
    let mut file = File::create(&out_path).expect("Failed to create output file");
    file.write_all(serde_json::to_string_pretty(&data).unwrap().as_bytes())
        .unwrap();

    println!("✔ Map generated: {:?}", out_path);
}
