use std::env;
use std::fs::File;
use std::path::PathBuf;
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use spin_snowball_shared::{CollisionMaskTag, ColorDef, MapObject, GoalDef};

#[derive(Deserialize)]
struct MapData {
    width: u32,
    height: u32,
    goals: Vec<GoalDef>,
    objects: Vec<MapObject>,
}

fn color_to_rgba(c: &ColorDef, mask: &[CollisionMaskTag], is_hole: bool) -> Rgba<u8> {
    let mut r = c.r.clamp(0, 255);
    let mut g = c.g.clamp(0, 255);
    let mut b = c.b.clamp(0, 255);
    let mut a = 255u8;

    if !is_hole {
        r &= !1;
        g &= !1;
        b &= !1;
        a &= !1;

        for tag in mask {
            match tag {
                CollisionMaskTag::Snowball => r |= 1,
                CollisionMaskTag::Ball => g |= 1,
                CollisionMaskTag::PlayerTeam1 => b |= 1,
                CollisionMaskTag::PlayerTeam2 => a |= 1,
            }
        }
    }

    Rgba([r, g, b, a])
}


fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: json_map_to_png <map.json> <output_folder>");
        std::process::exit(1);
    }

    let json_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out_dir).expect("Failed to create output folder");

    let file = File::open(&json_path).expect("Failed to open JSON file");
    let map: MapData = serde_json::from_reader(file).expect("Failed to parse JSON");

    let mut rects_img = RgbaImage::new(map.width, map.height);
    let mut circles_img = RgbaImage::new(map.width, map.height);
    let mut lines_img = RgbaImage::new(map.width, map.height);
    let mut goals_img = RgbaImage::new(map.width, map.height);

    for obj in &map.objects {
        match obj {
            MapObject::Rect { x, y, w, h, color, mask, is_hole, .. } => {
                let rgba = color_to_rgba(color, mask, *is_hole);
                for iy in *y as u32..(*y + *h) as u32 {
                    for ix in *x as u32..(*x + *w) as u32 {
                        rects_img.put_pixel(ix, iy, rgba);
                    }
                }
            }
            MapObject::Circle { x, y, radius, color, mask, is_hole, .. } => {
                let rgba = color_to_rgba(color, mask, *is_hole);
                let cx = *x;
                let cy = *y;
                for iy in 0..map.height {
                    for ix in 0..map.width {
                        let dx = ix as f32 - cx;
                        let dy = iy as f32 - cy;
                        if dx*dx + dy*dy <= *radius * *radius {
                            circles_img.put_pixel(ix, iy, rgba);
                        }
                    }
                }
            }
            MapObject::Line { ax, ay, bx, by, color, mask, is_hole, .. } => {
                let rgba = color_to_rgba(color, mask, *is_hole);
                let dx = bx - ax;
                let dy = by - ay;
                let steps = dx.abs().max(dy.abs()) as u32 + 1;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let ix = (ax + t * dx).round() as u32;
                    let iy = (ay + t * dy).round() as u32;
                    if ix < map.width && iy < map.height {
                        lines_img.put_pixel(ix, iy, rgba);
                    }
                }
            }
        }
    }

    // Goals
    for goal in &map.goals {
        let color = if matches!(goal.team, spin_snowball_shared::Team::Team1) {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        };
        for iy in goal.y as u32..(goal.y + goal.h) as u32 {
            for ix in goal.x as u32..(goal.x + goal.w) as u32 {
                goals_img.put_pixel(ix, iy, color);
            }
        }
    }

    rects_img.save(out_dir.join("rects.png")).unwrap();
    circles_img.save(out_dir.join("circles.png")).unwrap();
    lines_img.save(out_dir.join("lines.png")).unwrap();
    goals_img.save(out_dir.join("goals.png")).unwrap();

    println!("✔ PNG images generated in {:?}", out_dir);
}
