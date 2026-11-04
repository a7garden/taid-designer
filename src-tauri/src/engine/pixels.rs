use super::{Issue, Metrics, RuleProfile};
use image::{imageops, ImageFormat, ImageReader, Limits, Rgba, RgbaImage};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::BufReader,
    path::Path,
};

pub(super) const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub(super) const MAX_DIMENSION: u32 = 16384;

pub(super) fn dimensions_valid(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("이미지는 16메가픽셀 이하, 한 변은 16384px 이하여야 합니다.".into());
    }
    Ok(())
}

pub(super) fn load(path: &Path) -> Result<RgbaImage, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("파일을 읽을 수 없습니다 ({}): {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err("64 MiB 이하의 PNG 또는 JPEG 파일을 선택하세요.".into());
    }
    let reader = || -> Result<ImageReader<BufReader<fs::File>>, String> {
        let file =
            fs::File::open(path).map_err(|e| format!("이미지 파일을 열 수 없습니다: {e}"))?;
        let mut reader = ImageReader::new(BufReader::new(file))
            .with_guessed_format()
            .map_err(|e| format!("이미지 형식을 확인할 수 없습니다: {e}"))?;
        if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
            return Err("PNG와 JPEG 이미지만 지원합니다.".into());
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        Ok(reader)
    };
    let (width, height) = reader()?
        .into_dimensions()
        .map_err(|e| format!("이미지 크기를 읽을 수 없습니다: {e}"))?;
    dimensions_valid(width, height)?;
    let image = reader()?
        .decode()
        .map_err(|e| format!("이미지를 디코딩할 수 없습니다: {e}"))?
        .to_rgba8();
    dimensions_valid(image.width(), image.height())?;
    Ok(image)
}

pub(super) fn palette(profile: &RuleProfile) -> Result<Vec<[u8; 3]>, String> {
    profile
        .palette
        .iter()
        .map(|value| {
            let raw = value.strip_prefix('#').unwrap_or(value);
            if raw.len() != 6 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("팔레트 색상은 6자리 RGB 16진수(#RRGGBB)로 입력하세요.".into());
            }
            Ok([
                u8::from_str_radix(&raw[0..2], 16)
                    .map_err(|e| format!("팔레트 R 값을 읽을 수 없습니다: {e}"))?,
                u8::from_str_radix(&raw[2..4], 16)
                    .map_err(|e| format!("팔레트 G 값을 읽을 수 없습니다: {e}"))?,
                u8::from_str_radix(&raw[4..6], 16)
                    .map_err(|e| format!("팔레트 B 값을 읽을 수 없습니다: {e}"))?,
            ])
        })
        .collect()
}

fn visible(pixel: &Rgba<u8>) -> [u8; 4] {
    if pixel[3] == 0 {
        [0; 4]
    } else {
        pixel.0
    }
}

// Call only after the prescribed grid is aligned and every block is uniform.
// Sampling block origins avoids allocating another image or expanding a 4px dot
// into sixteen candidates. Matching RGB, not alpha, keeps mask edges from
// inventing extra color detail; fully transparent neighbors never match.
fn logical_dot_counts(image: &RgbaImage, grid: u32) -> (u64, u64) {
    let width = image.width() / grid;
    let height = image.height() / grid;
    let mut visible_dots = 0;
    let mut isolated_dots = 0;
    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x * grid, y * grid);
            if pixel[3] == 0 {
                continue;
            }
            visible_dots += 1;
            let mut matching_neighbor = false;
            'neighbors: for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    if nx == x && ny == y {
                        continue;
                    }
                    let neighbor = image.get_pixel(nx * grid, ny * grid);
                    if neighbor[3] > 0
                        && neighbor[0] == pixel[0]
                        && neighbor[1] == pixel[1]
                        && neighbor[2] == pixel[2]
                    {
                        matching_neighbor = true;
                        break 'neighbors;
                    }
                }
            }
            isolated_dots += u64::from(!matching_neighbor);
        }
    }
    (visible_dots, isolated_dots)
}

pub(super) fn inspect(
    image: &RgbaImage,
    profile: &RuleProfile,
) -> Result<(Vec<Issue>, Metrics), String> {
    let mut issues = Vec::new();
    let mut colors = HashSet::new();
    // RGB444 is a bounded 4096-bucket approximation, not perceptual distance:
    // nearby colors across a bucket boundary can be missed. Count extra unique
    // RGBs sharing a bucket rather than pixel frequency or all color pairs.
    let mut color_buckets = [0u64; 64];
    let mut occupied_buckets = 0u64;
    let allowed: HashSet<[u8; 3]> = palette(profile)?.into_iter().collect();
    let mut outside = 0;
    let mut metrics = Metrics::default();
    let mut edge = 0u64;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        if x == 0 || y == 0 || x + 1 == image.width() || y + 1 == image.height() {
            edge += 1;
        }
        metrics.opaque += u64::from(pixel[3] == 255);
        metrics.semi_transparent += u64::from(pixel[3] != 255);
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if colors.insert(rgb) {
            let bucket = (usize::from(rgb[0] >> 4) << 8)
                | (usize::from(rgb[1] >> 4) << 4)
                | usize::from(rgb[2] >> 4);
            let mask = 1u64 << (bucket % 64);
            let word = &mut color_buckets[bucket / 64];
            if *word & mask == 0 {
                *word |= mask;
                occupied_buckets += 1;
            }
        }
        if !allowed.is_empty() && !allowed.contains(&rgb) {
            outside += 1;
        }
    }
    metrics.colors = colors.len() as u64;
    metrics.near_color_variants = metrics.colors - occupied_buckets;
    if profile.width.is_some_and(|w| w != image.width())
        || profile.height.is_some_and(|h| h != image.height())
    {
        issues.push(Issue::error(
            "canvas",
            "캔버스 크기가 프로필에 지정된 크기와 다릅니다.",
            0,
        ));
    }
    if profile.alpha_mode == "binary" && metrics.semi_transparent > 0 {
        issues.push(Issue::error(
            "alpha",
            "알파가 0 또는 255여야 하지만 반투명 픽셀이 남아 있습니다.",
            metrics.semi_transparent,
        ));
    }
    let grid = profile.pixel_size;
    if grid > 1 {
        if !image.width().is_multiple_of(grid) || !image.height().is_multiple_of(grid) {
            issues.push(Issue::error(
                "grid-size",
                "캔버스 가로·세로는 지정된 픽셀 블록 크기의 배수여야 합니다.",
                0,
            ));
        }
        for by in (0..image.height()).step_by(grid as usize) {
            for bx in (0..image.width()).step_by(grid as usize) {
                let first = visible(image.get_pixel(bx, by));
                for y in by..(by + grid).min(image.height()) {
                    for x in bx..(bx + grid).min(image.width()) {
                        if visible(image.get_pixel(x, y)) != first {
                            metrics.grid_violations += 1;
                        }
                    }
                }
            }
        }
        if metrics.grid_violations > 0 {
            issues.push(Issue::error("grid", "좌상단 기준 블록 안의 RGBA가 균일하지 않습니다. 완전 투명 픽셀의 숨은 RGB는 무시합니다.", metrics.grid_violations));
        }
    }
    metrics.orphan_check_applied = image.width().is_multiple_of(grid)
        && image.height().is_multiple_of(grid)
        && metrics.grid_violations == 0;
    if metrics.orphan_check_applied {
        (metrics.visible_logical_dots, metrics.isolated_color_dots) =
            logical_dot_counts(image, grid);
    } else {
        issues.push(Issue::warning(
            "orphan-check-deferred",
            "픽셀아트라면 먼저 지정한 격자에 맞는지 확인하세요. 블록이 불균일하거나 캔버스가 맞지 않아 고립된 색 점 검사를 보류했습니다.",
            0,
        ));
    }
    // Advisory triage, never a palette limit: require both a substantial actual
    // palette and many approximate collisions (at least a quarter of its RGBs).
    // Smaller palettes still expose raw counts without warning on intentional
    // ramps; RGB444 cannot establish that two colors should be merged.
    if metrics.colors >= 64
        && metrics.near_color_variants >= 32
        && metrics.near_color_variants * 4 >= metrics.colors
    {
        issues.push(Issue::warning(
            "palette-variants",
            "픽셀아트라면 승인된 게임 팔레트와 비교해 보세요. RGB444 근사 집계에서 같은 구간의 RGB 변형이 많습니다. 색 수 자체는 오류가 아닙니다.",
            metrics.near_color_variants,
        ));
    }
    // At least 32 visible logical dots, four isolated candidates, and a 5%
    // share avoid nagging tiny sprites or a lone eye/highlight. These are review
    // thresholds, not proof of noise: eyes, stars, highlights and manual AA may
    // intentionally have no same-RGB neighbor. Never remove them here.
    if metrics.orphan_check_applied
        && metrics.visible_logical_dots >= 32
        && metrics.isolated_color_dots >= 4
        && metrics.isolated_color_dots * 20 >= metrics.visible_logical_dots
    {
        issues.push(Issue::warning(
            "isolated-dots",
            "픽셀아트라면 주변 8칸에 같은 RGB가 없는 논리 점을 확인하세요. 눈·별·하이라이트·수동 안티앨리어싱일 수 있으므로 자동 삭제하지 않습니다.",
            metrics.isolated_color_dots,
        ));
    }
    if outside > 0 {
        issues.push(Issue::error(
            "palette",
            "보이는 픽셀에 프로필 팔레트 밖의 색상이 있습니다.",
            outside,
        ));
    }
    if metrics.opaque + metrics.semi_transparent == 0 {
        issues.push(Issue::error("empty", "보이는 그림이 없는 이미지입니다.", 0));
    }
    if edge > 0 {
        issues.push(Issue::warning(
            "crop-edge",
            "그림이 캔버스 가장자리에 닿습니다. 의도하지 않은 잘림을 확인하세요.",
            edge,
        ));
    }
    Ok((issues, metrics))
}

pub(super) fn harden(image: &mut RgbaImage, threshold: u8) {
    for p in image.pixels_mut() {
        if p[3] == 0 || p[3] < threshold {
            *p = Rgba([0, 0, 0, 0]);
        } else {
            p[3] = 255;
        }
    }
}

pub(super) fn map_palette(image: &mut RgbaImage, colors: &[[u8; 3]]) {
    for p in image.pixels_mut().filter(|p| p[3] > 0) {
        if let Some(color) = colors.iter().min_by_key(|c| {
            (0..3)
                .map(|i| {
                    let d = i32::from(p[i]) - i32::from(c[i]);
                    d * d
                })
                .sum::<i32>()
        }) {
            p.0[..3].copy_from_slice(color);
        }
    }
}

pub(super) fn snap(image: &mut RgbaImage, grid: u32) {
    if grid == 1 {
        return;
    }
    let mut counts = HashMap::<[u8; 4], (u32, usize)>::new();
    for by in (0..image.height()).step_by(grid as usize) {
        for bx in (0..image.width()).step_by(grid as usize) {
            counts.clear();
            let mut index = 0;
            for y in by..(by + grid).min(image.height()) {
                for x in bx..(bx + grid).min(image.width()) {
                    let entry = counts
                        .entry(visible(image.get_pixel(x, y)))
                        .or_insert((0, index));
                    entry.0 += 1;
                    index += 1;
                }
            }
            let color = counts
                .iter()
                .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then_with(|| b.1 .1.cmp(&a.1 .1)))
                .map(|(p, _)| *p)
                .unwrap_or([0; 4]);
            for y in by..(by + grid).min(image.height()) {
                for x in bx..(bx + grid).min(image.width()) {
                    image.put_pixel(x, y, Rgba(color));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub x: u32,
    pub y: u32,
    pub right: u32,
    pub bottom: u32,
}

pub(super) fn bounds(image: &RgbaImage) -> Option<Bounds> {
    let mut bounds = Bounds {
        x: image.width(),
        y: image.height(),
        right: 0,
        bottom: 0,
    };
    for (x, y, p) in image.enumerate_pixels() {
        if p[3] > 0 {
            bounds.x = bounds.x.min(x);
            bounds.y = bounds.y.min(y);
            bounds.right = bounds.right.max(x + 1);
            bounds.bottom = bounds.bottom.max(y + 1);
        }
    }
    (bounds.right > bounds.x && bounds.bottom > bounds.y).then_some(bounds)
}

pub(super) fn crop(image: &RgbaImage, bounds: Bounds) -> RgbaImage {
    imageops::crop_imm(
        image,
        bounds.x,
        bounds.y,
        bounds.right - bounds.x,
        bounds.bottom - bounds.y,
    )
    .to_image()
}

pub(super) fn scale(image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
    let width = image
        .width()
        .checked_mul(factor)
        .ok_or("확대 후 가로 크기가 한도를 초과합니다")?;
    let height = image
        .height()
        .checked_mul(factor)
        .ok_or("확대 후 세로 크기가 한도를 초과합니다")?;
    dimensions_valid(width, height)?;
    Ok(imageops::resize(
        image,
        width,
        height,
        imageops::FilterType::Nearest,
    ))
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    fn profile(grid: u32) -> RuleProfile {
        let mut profile = super::super::default_profiles()[0].clone();
        profile.pixel_size = grid;
        profile
    }

    fn has_warning(issues: &[Issue], code: &str) -> bool {
        issues
            .iter()
            .any(|issue| issue.code == code && issue.severity == "warning")
    }

    #[test]
    fn opting_into_grid_cleanup_removes_noise_before_logical_dot_review() {
        let profile = profile(4);
        let mut image = RgbaImage::from_pixel(32, 32, Rgba([64, 96, 128, 255]));
        for i in 0..4 {
            image.put_pixel(i * 4 + 1, 5, Rgba([65 + i as u8, 97, 129, 255]));
        }
        let (issues, before) = inspect(&image, &profile).unwrap();
        assert_eq!(before.colors, 5);
        assert_eq!(before.near_color_variants, 4);
        assert_eq!(before.grid_violations, 4);
        assert!(!before.orphan_check_applied);
        assert!(has_warning(&issues, "orphan-check-deferred"));

        snap(&mut image, profile.pixel_size);
        let (issues, after) = inspect(&image, &profile).unwrap();
        assert_eq!(after.colors, 1);
        assert_eq!(after.near_color_variants, 0);
        assert_eq!(after.grid_violations, 0);
        assert!(after.orphan_check_applied);
        assert_eq!(after.visible_logical_dots, 64);
        assert_eq!(after.isolated_color_dots, 0);
        assert!(!has_warning(&issues, "orphan-check-deferred"));
        assert!(!issues.iter().any(|issue| issue.severity == "error"));
    }

    #[test]
    fn hidden_rgb_affects_neither_palette_grid_nor_dot_counts() {
        let mut profile = profile(4);
        profile.palette = vec!["#406080".into()];
        let mut image = RgbaImage::from_pixel(12, 12, Rgba([64, 96, 128, 0]));
        image.put_pixel(1, 1, Rgba([255, 17, 23, 0]));
        image.put_pixel(8, 8, Rgba([65, 97, 129, 0]));
        for y in 4..8 {
            for x in 4..8 {
                image.put_pixel(x, y, Rgba([64, 96, 128, 255]));
            }
        }
        let (issues, metrics) = inspect(&image, &profile).unwrap();
        assert_eq!(metrics.colors, 1);
        assert_eq!(metrics.near_color_variants, 0);
        assert_eq!(metrics.grid_violations, 0);
        assert!(metrics.orphan_check_applied);
        assert_eq!(metrics.visible_logical_dots, 1);
        assert_eq!(metrics.isolated_color_dots, 1);
        assert!(!issues.iter().any(|issue| issue.severity == "error"));
    }

    #[test]
    fn native_pixel_singleton_is_measured_without_failing_a_tiny_sprite() {
        let mut image = RgbaImage::new(3, 3);
        image.put_pixel(1, 1, Rgba([100, 120, 140, 255]));
        let (issues, metrics) = inspect(&image, &profile(1)).unwrap();
        assert!(metrics.orphan_check_applied);
        assert_eq!(metrics.visible_logical_dots, 1);
        assert_eq!(metrics.isolated_color_dots, 1);
        assert!(issues.is_empty());
    }

    #[test]
    fn four_pixel_grid_singleton_is_one_dot_not_sixteen() {
        let mut image = RgbaImage::new(12, 12);
        for y in 4..8 {
            for x in 4..8 {
                image.put_pixel(x, y, Rgba([100, 120, 140, 255]));
            }
        }
        let (issues, metrics) = inspect(&image, &profile(4)).unwrap();
        assert_eq!(metrics.opaque, 16);
        assert_eq!(metrics.visible_logical_dots, 1);
        assert_eq!(metrics.isolated_color_dots, 1);
        assert!(metrics.orphan_check_applied);
        assert!(issues.is_empty());
    }

    #[test]
    fn irregular_blocks_and_unaligned_dimensions_defer_dot_measurements() {
        let mut uneven = RgbaImage::from_pixel(8, 8, Rgba([40, 80, 120, 255]));
        uneven.put_pixel(1, 1, Rgba([41, 81, 121, 255]));
        let unaligned = RgbaImage::from_pixel(9, 8, Rgba([40, 80, 120, 255]));
        for (image, error_code, violations) in [(uneven, "grid", 1), (unaligned, "grid-size", 0)] {
            let (issues, metrics) = inspect(&image, &profile(4)).unwrap();
            assert!(!metrics.orphan_check_applied);
            assert_eq!(metrics.visible_logical_dots, 0);
            assert_eq!(metrics.isolated_color_dots, 0);
            assert_eq!(metrics.grid_violations, violations);
            assert!(has_warning(&issues, "orphan-check-deferred"));
            assert!(!has_warning(&issues, "isolated-dots"));
            assert!(issues
                .iter()
                .any(|issue| issue.code == error_code && issue.severity == "error"));
        }
    }

    #[test]
    fn effects_keep_alpha_and_diagonal_rgb_neighbors_are_not_color_noise() {
        let mut image = RgbaImage::new(4, 4);
        image.put_pixel(1, 1, Rgba([50, 100, 150, 40]));
        image.put_pixel(2, 2, Rgba([50, 100, 150, 200]));
        let mut effect = profile(1);
        effect.kind = "effect".into();
        let (issues, metrics) = inspect(&image, &effect).unwrap();
        assert_eq!(metrics.colors, 1);
        assert_eq!(metrics.near_color_variants, 0);
        assert_eq!(metrics.semi_transparent, 2);
        assert_eq!(metrics.visible_logical_dots, 2);
        assert_eq!(metrics.isolated_color_dots, 0);
        assert!(issues.is_empty());
        assert_eq!(image.get_pixel(1, 1)[3], 40);
        assert_eq!(image.get_pixel(2, 2)[3], 200);

        effect.alpha_mode = "binary".into();
        let (issues, _) = inspect(&image, &effect).unwrap();
        assert!(issues.iter().any(|issue| {
            issue.code == "alpha" && issue.severity == "error" && issue.pixels == 2
        }));
    }

    #[test]
    fn rgb444_variants_count_unique_colors_not_frequency_or_bucket_boundaries() {
        let mut image = RgbaImage::new(4, 1);
        for (x, rgb) in [[15, 0, 0], [16, 0, 0], [17, 0, 0], [17, 0, 0]]
            .into_iter()
            .enumerate()
        {
            image.put_pixel(x as u32, 0, Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
        let (issues, metrics) = inspect(&image, &profile(1)).unwrap();
        assert_eq!(metrics.colors, 3);
        assert_eq!(metrics.near_color_variants, 1);
        assert!(!has_warning(&issues, "palette-variants"));
    }

    #[test]
    fn substantial_palette_variation_is_an_advisory_not_a_color_limit() {
        for near_variants in [31u32, 32] {
            let mut image = RgbaImage::new(64, 1);
            for x in 0..64 {
                let (bucket, variant) = if x < near_variants * 2 {
                    (x / 2, x % 2)
                } else {
                    (x - near_variants, 0)
                };
                image.put_pixel(
                    x,
                    0,
                    Rgba([
                        ((bucket % 16) * 16) as u8,
                        ((bucket / 16) * 16) as u8,
                        variant as u8,
                        255,
                    ]),
                );
            }
            let (issues, metrics) = inspect(&image, &profile(1)).unwrap();
            assert_eq!(metrics.colors, 64);
            assert_eq!(metrics.near_color_variants, u64::from(near_variants));
            assert_eq!(
                has_warning(&issues, "palette-variants"),
                near_variants == 32
            );
            assert!(!issues.iter().any(|issue| issue.severity == "error"));
            if near_variants == 32 {
                assert!(issues
                    .iter()
                    .any(|issue| issue.code == "palette-variants" && issue.pixels == 32));
            }
        }
    }

    #[test]
    fn isolated_dot_advisories_respect_size_count_and_share_then_manual_cleanup() {
        for (visible, isolated, warn) in [
            (31, 4, false),
            (32, 3, false),
            (32, 4, true),
            (100, 4, false),
            (100, 5, true),
        ] {
            let base = Rgba([100, 120, 140, 255]);
            let mut image = RgbaImage::from_pixel(visible, 1, base);
            for i in 0..isolated {
                image.put_pixel(2 + i * 3, 0, Rgba([i as u8, 20, 40, 255]));
            }
            let (issues, metrics) = inspect(&image, &profile(1)).unwrap();
            assert_eq!(metrics.visible_logical_dots, u64::from(visible));
            assert_eq!(metrics.isolated_color_dots, u64::from(isolated));
            assert_eq!(has_warning(&issues, "isolated-dots"), warn);
            assert!(!issues.iter().any(|issue| issue.severity == "error"));
            if warn {
                assert!(issues.iter().any(|issue| {
                    issue.code == "isolated-dots" && issue.pixels == u64::from(isolated)
                }));
            }

            // This explicit edit represents the artist's cleanup choice.
            for i in 0..isolated {
                image.put_pixel(2 + i * 3, 0, base);
            }
            let (issues, metrics) = inspect(&image, &profile(1)).unwrap();
            assert_eq!(metrics.visible_logical_dots, u64::from(visible));
            assert_eq!(metrics.isolated_color_dots, 0);
            assert!(!has_warning(&issues, "isolated-dots"));
        }
    }
}
