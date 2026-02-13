use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::Pixel;
use ndarray::Array2;
use serde::Serialize;
use spin_snowball_shared::{CollisionMaskTag, ColorDef, GoalDef, MapObject, Team};

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

fn color_mask(data: &[RGBA], width: u32, height: u32) -> Array2<u32> {
    let mut mask = Array2::<u32>::zeros((height as usize, width as usize));
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = y * width as usize + x;
            let px = &data[idx];
            if px.a > 0 {
                mask[(y, x)] =
                    (px.r as u32) << 24 |
                    (px.g as u32) << 16 |
                    (px.b as u32) << 8  |
                    (px.a as u32);
            }
        }
    }
    mask
}

fn bit_mask(c: u8) -> bool {
    (c & 1) == 1
}

fn decode_rgb(r: u8, g: u8, b: u8, a: u8) -> (bool, Vec<CollisionMaskTag>) {
    let mut mask = Vec::new();

    if bit_mask(r) {
        mask.push(CollisionMaskTag::Snowball);
    }
    if bit_mask(g) {
        mask.push(CollisionMaskTag::Ball);
    }
    if bit_mask(b) {
        mask.push(CollisionMaskTag::Team1);
    }
    if bit_mask(a) {
        mask.push(CollisionMaskTag::Team2);
    }
    let is_hole = r == 127 && b == 127 && g == 127;

    (is_hole, mask)
}

fn strip_mask_bit(c: u8) -> u8 {
    let base = c & 0b1111_1110; // clear LSB
    base
}

fn label_components(mask: &Array2<u32>) -> (Array2<i32>, i32) {
    use std::collections::VecDeque;

    let (h, w) = mask.dim();
    let mut labels = Array2::<i32>::zeros((h, w));
    let mut current_label = 0;

    for y in 0..h {
        for x in 0..w {
            let color = mask[(y, x)];
            if color == 0 || labels[(y, x)] != 0 {
                continue;
            }

            current_label += 1;
            let mut queue = VecDeque::new();
            queue.push_back((x, y));

            while let Some((cx, cy)) = queue.pop_front() {
                if cx >= w || cy >= h {
                    continue;
                }
                if labels[(cy, cx)] != 0 || mask[(cy, cx)] != color {
                    continue;
                }

                labels[(cy, cx)] = current_label;

                if cx > 0 { queue.push_back((cx - 1, cy)); }
                if cx + 1 < w { queue.push_back((cx + 1, cy)); }
                if cy > 0 { queue.push_back((cx, cy - 1)); }
                if cy + 1 < h { queue.push_back((cx, cy + 1)); }
            }
        }
    }

    (labels, current_label)
}

fn extract_rectangles(data: &[RGBA], width: u32, height: u32) -> Vec<MapObject> {
    let mask = color_mask(data, width, height);
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

        objects.push(MapObject::Rect {
                x: x0 as f32,
                y: y0 as f32,
                w: (x1 - x0 + 1) as f32,
                h: (y1 - y0 + 1) as f32,
                is_hole,
                factor: 1.0,
                color: ColorDef {
                    r: strip_mask_bit(px.r),
                    g: strip_mask_bit(px.g),
                    b: strip_mask_bit(px.b),
                    a: 255,
                },
                mask: if is_hole {
                    vec![]
                } else {
                    mask
                },
            },
        );
    }

    objects
}

fn extract_circles(data: &[RGBA], width: u32, height: u32) -> Vec<MapObject> {
    let mask = color_mask(data, width, height);
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

        objects.push(MapObject::Circle {
                x: cx,
                y: cy,
                radius,
                is_hole,
                factor: 1.0,
                color: ColorDef {
                    r: strip_mask_bit(px.r),
                    g: strip_mask_bit(px.g),
                    b: strip_mask_bit(px.b),
                    a: 255,
                },
                mask: if is_hole {
                    vec![]
                } else {
                    mask
                },
            },
        );
    }

    objects
}

fn extract_goals(data: &[RGBA], width: u32, height: u32) -> Vec<GoalDef> {
    let mask = color_mask(data, width, height);
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

        let team = if px.r == 255 { Team::Team1 } else { Team::Team2 };

        goals.push(GoalDef {
            x: x0 as f32,
            y: y0 as f32,
            w: (x1 - x0 + 1) as f32,
            h: (y1 - y0 + 1) as f32,
            team
        });
    }

    goals
}

fn find_farthest_endpoints(
    pixels: &[(usize, usize)]
) -> ((usize, usize), (usize, usize)) {
    let mut max_dist = 0i32;
    let mut best = (pixels[0], pixels[0]);

    for &p1 in pixels {
        for &p2 in pixels {
            let dx = p1.0 as i32 - p2.0 as i32;
            let dy = p1.1 as i32 - p2.1 as i32;
            let d = dx * dx + dy * dy;
            if d > max_dist {
                max_dist = d;
                best = (p1, p2);
            }
        }
    }

    best
}

fn extract_lines(data: &[RGBA], width: u32, height: u32) -> Vec<MapObject> {
    use std::collections::VecDeque;

    // Create a mask of u32 where each pixel's value is its RGB combined
    let mut mask = Array2::<u32>::zeros((height as usize, width as usize));
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = y * width as usize + x;
            let px = &data[idx];
            if px.a > 0 {
                mask[(y, x)] = (px.r as u32) << 16 | (px.g as u32) << 8 | (px.b as u32);
            }
        }
    }

    let mut labels = Array2::<i32>::zeros((height as usize, width as usize));
    let mut current_label = 0;
    let mut objects = Vec::new();

    for y in 0..height as usize {
        for x in 0..width as usize {
            if mask[(y, x)] != 0 && labels[(y, x)] == 0 {
                current_label += 1;
                let color = mask[(y, x)];
                let mut queue = VecDeque::new();
                let mut pixels = Vec::new();
                queue.push_back((x, y));

                while let Some((cx, cy)) = queue.pop_front() {
                    if cx >= width as usize || cy >= height as usize {
                        continue;
                    }
                    if labels[(cy, cx)] != 0 || mask[(cy, cx)] != color {
                        continue;
                    }
                    labels[(cy, cx)] = current_label;
                    pixels.push((cx, cy));

                    // 4-connected neighbors
                    if cx > 0 {
                        queue.push_back((cx - 1, cy));
                    }
                    if cx + 1 < width as usize {
                        queue.push_back((cx + 1, cy));
                    }
                    if cy > 0 {
                        queue.push_back((cx, cy - 1));
                    }
                    if cy + 1 < height as usize {
                        queue.push_back((cx, cy + 1));
                    }
                }

                // find farthest endpoints for this color component
                let ((ax, ay), (bx, by)) = find_farthest_endpoints(&pixels);
                let idx = ay * width as usize + ax;
                let px = data[idx];

                let (is_hole, mask_bits) = decode_rgb(px.r, px.g, px.b, px.a);

                objects.push(MapObject::Line {
                        ax: ax as f32,
                        ay: ay as f32,
                        bx: bx as f32,
                        by: by as f32,
                        is_hole,
                        factor: 1.0,
                        color: ColorDef {
                            r: strip_mask_bit(px.r),
                            g: strip_mask_bit(px.g),
                            b: strip_mask_bit(px.b),
                            a: 255,
                        },
                        mask: if is_hole {
                            vec![]
                        } else {
                            mask_bits
                        },
                    },
                );
            }
        }
    }

    objects
}

#[derive(Serialize)]
struct MapData {
    name: String,
    width: u32,
    height: u32,
    physics: serde_json::Value,
    team1: serde_json::Value,
    team2: serde_json::Value,
    ball: serde_json::Value,
    goals: Vec<GoalDef>,
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
    let (lines, _, _) = load_rgba(&map_dir.join("lines.png"));


    let mut objects = Vec::new();

    for r in extract_rectangles(&rects, w, h) {
        objects.push(serde_json::to_value(r).unwrap());
    }
    for c in extract_circles(&circles, w, h) {
        objects.push(serde_json::to_value(c).unwrap());
    }
    for l in extract_lines(&lines, w, h) {
        objects.push(serde_json::to_value(l).unwrap());
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
