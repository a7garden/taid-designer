use super::variants::PreparedVariant;
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteColor {
    pub hex: String,
    pub pixels: u64,
    /// Fraction of visible physical pixels, including partially transparent pixels.
    pub frequency: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteOutside {
    #[serde(flatten)]
    pub color: PaletteColor,
    pub nearest_approved: Option<String>,
    pub distance_squared: Option<u32>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteReport {
    pub asset_id: String,
    pub input_hash: String,
    pub visible_pixels: u64,
    pub color_count: u64,
    pub approved_palette_size: usize,
    pub outside_colors: u64,
    pub outside_pixels: u64,
    pub dominant_colors: Vec<PaletteColor>,
    pub outside_samples: Vec<PaletteOutside>,
    pub all_colors: Option<Vec<PaletteColor>>,
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| {
            let difference = i32::from(a) - i32::from(b);
            (difference * difference) as u32
        })
        .sum()
}
fn nearest(rgb: [u8; 3], approved: &[[u8; 3]]) -> Option<([u8; 3], u32)> {
    approved
        .iter()
        .map(|&candidate| (candidate, distance(rgb, candidate)))
        .min_by_key(|&(candidate, distance)| (distance, candidate))
}
fn histogram(image: &RgbaImage) -> HashMap<[u8; 3], u64> {
    let mut counts = HashMap::new();
    for pixel in image.pixels().filter(|pixel| pixel[3] != 0) {
        *counts.entry([pixel[0], pixel[1], pixel[2]]).or_default() += 1;
    }
    counts
}
fn report(
    image: &RgbaImage,
    profile: &RuleProfile,
    asset_id: String,
    input_hash: String,
) -> Result<PaletteReport> {
    let approved = pixels::palette(profile)?;
    let allowed: HashSet<_> = approved.iter().copied().collect();
    let mut counts: Vec<_> = histogram(image).into_iter().collect();
    counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let visible_pixels: u64 = counts.iter().map(|(_, count)| count).sum();
    let color = |&(rgb, count): &([u8; 3], u64)| PaletteColor {
        hex: hex(rgb),
        pixels: count,
        frequency: if visible_pixels == 0 {
            0.0
        } else {
            count as f64 / visible_pixels as f64
        },
    };
    let outside: Vec<_> = counts
        .iter()
        .filter(|(rgb, _)| !allowed.is_empty() && !allowed.contains(rgb))
        .collect();
    Ok(PaletteReport {
        asset_id,
        input_hash,
        visible_pixels,
        color_count: counts.len() as u64,
        approved_palette_size: allowed.len(),
        outside_colors: outside.len() as u64,
        outside_pixels: outside.iter().map(|(_, count)| count).sum(),
        dominant_colors: counts.iter().take(32).map(color).collect(),
        outside_samples: outside
            .iter()
            .take(24)
            .map(|entry| {
                let suggestion = nearest(entry.0, &approved);
                PaletteOutside {
                    color: color(entry),
                    nearest_approved: suggestion.map(|(rgb, _)| hex(rgb)),
                    distance_squared: suggestion.map(|(_, distance)| distance),
                }
            })
            .collect(),
        all_colors: (counts.len() <= 256).then(|| counts.iter().map(color).collect()),
    })
}
fn map(image: &mut RgbaImage, approved: &[[u8; 3]]) -> Result<()> {
    if approved.is_empty() {
        return Err(
            "저장된 제작 기준에 승인 팔레트가 없습니다. 먼저 실제 게임 팔레트를 지정하세요.".into(),
        );
    }
    let mut cache = HashMap::<[u8; 3], [u8; 3]>::new();
    for pixel in image.pixels_mut().filter(|pixel| pixel[3] != 0) {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let mapped = *cache
            .entry(rgb)
            .or_insert_with(|| nearest(rgb, approved).expect("nonempty palette").0);
        pixel.0[..3].copy_from_slice(&mapped);
    }
    Ok(())
}

impl EngineState {
    pub fn asset_palette(&self, asset_id: &str) -> Result<PaletteReport> {
        let state = self.lock()?;
        let project = state.project.as_ref().ok_or("먼저 작업 폴더를 여세요.")?;
        let _lock = WorkspaceLock::acquire(&project.root)?;
        project.check_revision()?;
        let asset = &project.document.assets[project.index(asset_id)?].asset;
        let path = confined(&project.root, &asset.file_path)?;
        if hash_file(&path)? != asset.current_hash {
            return Err("검사할 현재 이미지가 변경되었습니다. 프로젝트를 다시 불러오세요.".into());
        }
        let image = pixels::load(&path)?;
        if hash_file(&path)? != asset.current_hash
            || image.dimensions() != (asset.width, asset.height)
        {
            return Err("검사 중 이미지가 변경되었습니다. 프로젝트를 다시 불러오세요.".into());
        }
        report(
            &image,
            project.profile(&asset.profile_id)?,
            asset.id.clone(),
            asset.current_hash.clone(),
        )
    }

    pub fn map_palette_assets(&self, ids: &[String]) -> Result<Workspace> {
        let snapshot = self.prepare_variants(ids)?;
        self.add_variants(&snapshot, "palette", |index, parent| {
            let approved = pixels::palette(snapshot.profile(index))?;
            let mut image = snapshot.load_image(index)?;
            map(&mut image, &approved)?;
            Ok(PreparedVariant {
                image, profile: None, pivot: (parent.pivot_x, parent.pivot_y),
                provenance: serde_json::json!({"palette": snapshot.profile(index).palette, "distance": "squared-rgb"}),
            })
        })
    }
}

/// Parse real plaintext HEX or GIMP palettes. ASE is binary and intentionally not accepted.
pub fn read_palette_file(path: &Path) -> Result<Vec<String>> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "hex" | "gpl") {
        return Err("텍스트 .hex 또는 GIMP .gpl 팔레트 파일을 선택하세요. ASE 바이너리는 지원하지 않습니다.".into());
    }
    const LIMIT: u64 = 1024 * 1024;
    let metadata = fs::metadata(path).map_err(|e| err("팔레트 파일을 읽을 수 없습니다", e))?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("팔레트 파일은 1 MiB 이하의 일반 텍스트 파일이어야 합니다.".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| err("팔레트 파일을 열 수 없습니다", e))?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| err("팔레트 파일을 읽을 수 없습니다", e))?;
    if bytes.len() as u64 > LIMIT {
        return Err("팔레트 파일이 1 MiB를 초과합니다.".into());
    }
    let content = String::from_utf8(bytes).map_err(|_| "팔레트 파일은 UTF-8 텍스트여야 합니다.")?;
    parse_palette(&content, extension == "gpl")
}
fn parse_palette(content: &str, gpl: bool) -> Result<Vec<String>> {
    let mut colors = Vec::new();
    let mut seen = HashSet::new();
    let mut header = false;
    for (index, raw) in content.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            continue;
        }
        let color = if gpl {
            if line == "GIMP Palette" {
                header = true;
                continue;
            }
            if line.starts_with('#') || line.starts_with("Name:") || line.starts_with("Columns:") {
                continue;
            }
            if !header {
                return Err("GPL 파일은 GIMP Palette 헤더로 시작해야 합니다.".into());
            }
            let mut channels = line.split_whitespace();
            let mut rgb = [0; 3];
            for channel in &mut rgb {
                *channel = channels
                    .next()
                    .and_then(|value| value.parse::<u8>().ok())
                    .ok_or_else(|| {
                        format!("팔레트 {}행: RGB 값은 0~255 정수여야 합니다.", index + 1)
                    })?;
            }
            hex(rgb)
        } else {
            let body = line
                .split("//")
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if body.starts_with('#') && !body.split_whitespace().next().is_some_and(valid_hex) {
                continue;
            }
            for token in body
                .split(|character: char| character.is_whitespace() || character == ',')
                .filter(|token| !token.is_empty())
            {
                if token.starts_with('#') && !valid_hex(token) {
                    break;
                }
                add_color(token, &mut colors, &mut seen)?;
            }
            continue;
        };
        add_color(&color, &mut colors, &mut seen)?;
    }
    if colors.is_empty() {
        return Err("팔레트 파일에 RGB 색상이 없습니다.".into());
    }
    // Normalize through the same RGB parser used by profile validation and QC.
    let mut profile = default_profiles().remove(0);
    profile.palette = colors;
    Ok(pixels::palette(&profile)?.into_iter().map(hex).collect())
}
fn valid_hex(value: &str) -> bool {
    let raw = value.strip_prefix('#').unwrap_or(value);
    raw.len() == 6 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn add_color(value: &str, colors: &mut Vec<String>, seen: &mut HashSet<String>) -> Result<()> {
    if !valid_hex(value) {
        return Err("팔레트 색상은 6자리 RGB 16진수(#RRGGBB)로 입력하세요.".into());
    }
    let color = format!(
        "#{}",
        value
            .strip_prefix('#')
            .unwrap_or(value)
            .to_ascii_uppercase()
    );
    if seen.insert(color.clone()) {
        colors.push(color);
    }
    if colors.len() > 256 {
        return Err("승인 팔레트는 서로 다른 RGB 색상을 최대 256개까지 지정할 수 있습니다.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn histogram_counts_visible_rgb_not_hidden_colors_or_alpha_variants() {
        let mut image = RgbaImage::from_pixel(4, 1, Rgba([90, 100, 110, 0]));
        image.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
        image.put_pixel(1, 0, Rgba([1, 2, 3, 128]));
        image.put_pixel(2, 0, Rgba([9, 8, 7, 255]));
        let mut profile = default_profiles().remove(0);
        profile.palette = vec!["#000000".into(), "#010203".into()];
        let result = report(&image, &profile, "id".into(), "hash".into()).unwrap();
        assert_eq!(
            (
                result.visible_pixels,
                result.color_count,
                result.outside_colors,
                result.outside_pixels
            ),
            (3, 2, 1, 1)
        );
        assert_eq!(result.dominant_colors[0].pixels, 2);
        assert_eq!(result.dominant_colors[0].frequency, 2.0 / 3.0);
        assert_eq!(
            result.outside_samples[0].nearest_approved.as_deref(),
            Some("#010203")
        );
        assert_eq!(result.outside_samples[0].distance_squared, Some(116));
    }

    #[test]
    fn rich_image_report_is_bounded_and_unconfigured_is_not_off_palette() {
        let image = RgbaImage::from_fn(300, 1, |x, _| {
            Rgba([(x % 256) as u8, (x / 256) as u8, 2, 255])
        });
        let profile = default_profiles().remove(0);
        let result = report(&image, &profile, "id".into(), "hash".into()).unwrap();
        assert_eq!(result.color_count, 300);
        assert_eq!(result.dominant_colors.len(), 32);
        assert!(result.all_colors.is_none());
        assert_eq!(result.outside_colors, 0);
        assert_eq!(result.approved_palette_size, 0);
        let mut approved = profile.clone();
        approved.palette = vec!["#FFFFFF".into()];
        let outside = report(&image, &approved, "id".into(), "hash".into()).unwrap();
        assert_eq!(outside.outside_colors, 300);
        assert_eq!(outside.outside_samples.len(), 24);
        let exact = image::imageops::crop_imm(&image, 0, 0, 256, 1).to_image();
        assert_eq!(
            report(&exact, &profile, "id".into(), "hash".into())
                .unwrap()
                .all_colors
                .unwrap()
                .len(),
            256
        );
    }

    #[test]
    fn palette_mapping_preserves_alpha_and_hidden_rgb_and_uses_only_approved_rgb() {
        let mut image = RgbaImage::from_fn(5, 1, |x, _| Rgba([x as u8 * 40, 10, 20, x as u8 * 50]));
        let before = image.clone();
        let approved = [[0, 0, 0], [255, 255, 255]];
        map(&mut image, &approved).unwrap();
        assert_eq!(image.get_pixel(0, 0), before.get_pixel(0, 0));
        for (before, after) in before.pixels().zip(image.pixels()).skip(1) {
            assert_eq!(before[3], after[3]);
            assert!(approved.contains(&[after[0], after[1], after[2]]));
        }
        assert!(map(&mut image, &[]).is_err());
    }

    #[test]
    fn reads_real_hex_and_gpl_headers_comments_duplicates_and_rejects_bad_rgb() {
        assert_eq!(
            parse_palette(
                "; comment\nFF0000\n#00ff00 // green\n# notes\nff0000",
                false
            )
            .unwrap(),
            ["#FF0000", "#00FF00"]
        );
        assert_eq!(parse_palette("GIMP Palette\nName: Real palette\nColumns: 2\n# comment\n255 0 0 Red\n0 255 0 Green", true).unwrap(), ["#FF0000", "#00FF00"]);
        assert!(parse_palette("GIMP Palette\n300 0 0 Bad", true).is_err());
        assert!(parse_palette("not a palette", false).is_err());
        let rich = (0..257)
            .map(|index| format!("{:06X}", index))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_palette(&rich, false).is_err());
    }
}
