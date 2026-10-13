use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn create_ico(path: &Path, is_enabled: bool) {
    let width: u32 = 32;
    let height: u32 = 32;

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    // Colors: BGRA
    // Enabled: Vibrant cyan/blue (#0088FF)
    // Disabled: Slate Gray (#808080)
    let (b, g, r): (u8, u8, u8) = if is_enabled {
        (0xFF, 0x88, 0x00) // Blue/Cyan
    } else {
        (0x80, 0x80, 0x80) // Gray
    };

    // Draw a stylized Wi-Fi / signal icon onto the 32x32 canvas
    let center_x = 16.0f32;
    let center_y = 6.0f32; // In bottom-up BMP, y=0 is bottom, so base dot is at y=6

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            let mut in_icon = false;

            // 1. Base dot: circle at (16, 6) with radius ~3
            if dist <= 3.0 && dy >= -1.0 {
                in_icon = true;
            }

            // 2. Arc 1: radius 7 to 9.5, within angle cone (dy > 0 and |dx| <= dy * 1.6)
            if dist >= 6.5 && dist <= 9.5 && dy > 1.0 && dx.abs() <= dy * 1.5 {
                in_icon = true;
            }

            // 3. Arc 2: radius 13 to 16, within angle cone
            if dist >= 13.0 && dist <= 16.0 && dy > 2.0 && dx.abs() <= dy * 1.5 {
                in_icon = true;
            }

            // 4. Arc 3: radius 19.5 to 22.5
            if dist >= 19.5 && dist <= 22.5 && dy > 3.0 && dx.abs() <= dy * 1.5 {
                in_icon = true;
            }

            let idx = ((y * width + x) * 4) as usize;
            if in_icon {
                pixels[idx] = b;
                pixels[idx + 1] = g;
                pixels[idx + 2] = r;
                pixels[idx + 3] = 255; // Alpha
            } else {
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
                pixels[idx + 3] = 0; // Transparent
            }
        }
    }

    // BMP mask: 1 bit per pixel, row padded to 32 bits (4 bytes). 32 bits = 4 bytes per row.
    let and_mask = vec![0u8; (width / 8 * height) as usize];

    let header_size = 6u32;
    let dir_entry_size = 16u32;
    let bmi_header_size = 40u32;
    let image_data_size = (pixels.len() + and_mask.len()) as u32;
    let total_image_size = bmi_header_size + image_data_size;

    let mut file_bytes = Vec::new();

    // ICO header
    file_bytes.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    file_bytes.extend_from_slice(&1u16.to_le_bytes()); // Type: 1 = Icon
    file_bytes.extend_from_slice(&1u16.to_le_bytes()); // Count: 1 image

    // Directory entry
    file_bytes.push(width as u8);
    file_bytes.push(height as u8);
    file_bytes.push(0); // Colors
    file_bytes.push(0); // Reserved
    file_bytes.extend_from_slice(&1u16.to_le_bytes()); // Planes
    file_bytes.extend_from_slice(&32u16.to_le_bytes()); // BPP
    file_bytes.extend_from_slice(&total_image_size.to_le_bytes()); // Size of image data
    let offset = header_size + dir_entry_size;
    file_bytes.extend_from_slice(&offset.to_le_bytes()); // Offset

    // BITMAPINFOHEADER
    file_bytes.extend_from_slice(&bmi_header_size.to_le_bytes());
    file_bytes.extend_from_slice(&(width as i32).to_le_bytes());
    file_bytes.extend_from_slice(&((height * 2) as i32).to_le_bytes()); // Height * 2 for icon
    file_bytes.extend_from_slice(&1u16.to_le_bytes()); // Planes
    file_bytes.extend_from_slice(&32u16.to_le_bytes()); // BitCount
    file_bytes.extend_from_slice(&0u32.to_le_bytes()); // Compression: BI_RGB
    file_bytes.extend_from_slice(&(pixels.len() as u32).to_le_bytes()); // SizeImage
    file_bytes.extend_from_slice(&0i32.to_le_bytes()); // XPelsPerMeter
    file_bytes.extend_from_slice(&0i32.to_le_bytes()); // YPelsPerMeter
    file_bytes.extend_from_slice(&0u32.to_le_bytes()); // ClrUsed
    file_bytes.extend_from_slice(&0u32.to_le_bytes()); // ClrImportant

    // Pixel data (BGRA)
    file_bytes.extend_from_slice(&pixels);

    // AND mask
    file_bytes.extend_from_slice(&and_mask);

    let mut f = File::create(path).expect("Failed to create icon file");
    f.write_all(&file_bytes).expect("Failed to write icon file");
}

fn main() {
    let assets_dir = Path::new("assets");
    let _ = fs::create_dir_all(assets_dir);

    let enabled_icon = assets_dir.join("icon_enabled.ico");
    let disabled_icon = assets_dir.join("icon_disabled.ico");

    create_ico(&enabled_icon, true);
    create_ico(&disabled_icon, false);

    // Embed resource for Windows
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/app_logo.ico");
        res.set("FileDescription", "Wi-Fi Proxy Auto-Switcher");
        res.set("ProductName", "WifiProxySwitcher");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Failed to compile Windows resources: {}", e);
        }
    }
}
