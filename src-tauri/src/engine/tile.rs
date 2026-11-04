use super::variants::PreparedVariant;
use super::*;
use image::{imageops, Rgba};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TileMode {
    Pad,
    Fit,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TileAnchor {
    Center,
    BottomCenter,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileSettings {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub mode: TileMode,
    pub anchor: TileAnchor,
}
impl TileSettings {
    fn dimensions(&self) -> Result<(u32, u32)> {
        if !(1..=64).contains(&self.scale) {
            return Err("출력 도트 배율은 1~64 정수여야 합니다.".into());
        }
        let width = self
            .width
            .checked_mul(self.scale)
            .ok_or("출력 너비가 한도를 초과합니다.")?;
        let height = self
            .height
            .checked_mul(self.scale)
            .ok_or("출력 높이가 한도를 초과합니다.")?;
        pixels::dimensions_valid(width, height)?;
        Ok((width, height))
    }
}
fn visible(pixel: &Rgba<u8>) -> Rgba<u8> {
    if pixel[3] == 0 {
        Rgba([0; 4])
    } else {
        *pixel
    }
}

/// A tile transform starts only from an already aligned, uniform logical grid.
/// Transparent hidden RGB is not artwork and does not invalidate a block.
fn logical_image(image: &RgbaImage, grid: u32) -> Result<RgbaImage> {
    let grid_error = || {
        "입력 캔버스는 현재 제작 기준의 도트 크기 배수이고 각 도트 블록이 균일해야 합니다. 먼저 제작 기준의 도트 크기를 확인하고 격자 정리를 적용하세요.".to_string()
    };
    if grid == 0 || !image.width().is_multiple_of(grid) || !image.height().is_multiple_of(grid) {
        return Err(grid_error());
    }
    let mut logical = RgbaImage::new(image.width() / grid, image.height() / grid);
    for (x, y, output) in logical.enumerate_pixels_mut() {
        let sample = visible(image.get_pixel(x * grid, y * grid));
        for by in y * grid..(y + 1) * grid {
            for bx in x * grid..(x + 1) * grid {
                if visible(image.get_pixel(bx, by)) != sample {
                    return Err(grid_error());
                }
            }
        }
        *output = sample;
    }
    Ok(logical)
}

fn fitted_dimensions(source: (u32, u32), settings: &TileSettings) -> Result<(u32, u32)> {
    let (width, height) = source;
    if settings.mode == TileMode::Pad {
        if width > settings.width || height > settings.height {
            return Err(format!("입력은 {width}×{height}도트로 {}×{}도트 캔버스보다 큽니다. 자르지 않으려면 더 큰 캔버스 또는 맞춤 축소를 선택하세요.", settings.width, settings.height));
        }
        return Ok(source);
    }
    if width <= settings.width && height <= settings.height {
        let factor = (settings.width / width).min(settings.height / height);
        return Ok((width * factor, height * factor));
    }
    // A rational aspect ratio avoids intermediate floating-point rounding. Round
    // the other dimension to the nearest whole dot (at least 1, inside canvas).
    if u64::from(settings.width) * u64::from(height)
        <= u64::from(settings.height) * u64::from(width)
    {
        Ok((
            settings.width,
            ((u64::from(height) * u64::from(settings.width) + u64::from(width) / 2)
                / u64::from(width))
            .max(1) as u32,
        ))
    } else {
        Ok((
            ((u64::from(width) * u64::from(settings.height) + u64::from(height) / 2)
                / u64::from(height))
            .max(1) as u32,
            settings.height,
        ))
    }
}
pub(super) fn render(
    image: &RgbaImage,
    grid: u32,
    pivot: (f64, f64),
    settings: &TileSettings,
) -> Result<(RgbaImage, (f64, f64))> {
    let (physical_width, physical_height) = settings.dimensions()?;
    let logical = logical_image(image, grid)?;
    let (width, height) = fitted_dimensions(logical.dimensions(), settings)?;
    let offset_x = (settings.width - width) / 2;
    let offset_y = match settings.anchor {
        TileAnchor::Center => (settings.height - height) / 2,
        TileAnchor::BottomCenter => settings.height - height,
    };
    let ratio_x = f64::from(width) / f64::from(logical.width());
    let ratio_y = f64::from(height) / f64::from(logical.height());
    let resized = if logical.dimensions() == (width, height) {
        logical
    } else {
        imageops::resize(&logical, width, height, imageops::FilterType::Nearest)
    };
    let mut output = RgbaImage::new(physical_width, physical_height);
    // Copy each logical sample directly to its integer physical block. No filter
    // ever introduces a color or an intermediate alpha value.
    for (x, y, pixel) in resized.enumerate_pixels() {
        for by in (offset_y + y) * settings.scale..(offset_y + y + 1) * settings.scale {
            for bx in (offset_x + x) * settings.scale..(offset_x + x + 1) * settings.scale {
                output.put_pixel(bx, by, *pixel);
            }
        }
    }
    let pivot = (
        (pivot.0 / f64::from(grid) * ratio_x + f64::from(offset_x)) * f64::from(settings.scale),
        (pivot.1 / f64::from(grid) * ratio_y + f64::from(offset_y)) * f64::from(settings.scale),
    );
    Ok((output, pivot))
}

impl EngineState {
    pub fn tile_assets(&self, ids: &[String], settings: &TileSettings) -> Result<Workspace> {
        let (width, height) = settings.dimensions()?;
        let snapshot = self.prepare_variants(ids)?;
        let mut profiles = HashMap::<String, RuleProfile>::new();
        for index in 0..snapshot.assets.len() {
            let parent = snapshot.profile(index);
            profiles.entry(parent.id.clone()).or_insert_with(|| {
                let mut profile = parent.clone();
                profile.id = Uuid::new_v4().to_string();
                profile.name = format!(
                    "{} · {}×{}도트 @{}x",
                    variants::bounded_label(&parent.name, 150),
                    settings.width,
                    settings.height,
                    settings.scale
                );
                profile.pixel_size = settings.scale;
                profile.width = Some(width);
                profile.height = Some(height);
                profile
            });
        }
        self.add_variants(&snapshot, "tile", |index, parent| {
            let source_profile = snapshot.profile(index);
            let image = snapshot.load_image(index)?;
            let (image, pivot) = render(&image, source_profile.pixel_size, (parent.pivot_x, parent.pivot_y), settings)?;
            Ok(PreparedVariant {
                image, profile: profiles.get(&source_profile.id).cloned(), pivot,
                provenance: serde_json::json!({"tile": settings, "inputPixelSize": source_profile.pixel_size, "parentProfileId": source_profile.id}),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings(
        width: u32,
        height: u32,
        scale: u32,
        mode: TileMode,
        anchor: TileAnchor,
    ) -> TileSettings {
        TileSettings {
            width,
            height,
            scale,
            mode,
            anchor,
        }
    }

    #[test]
    fn padding_anchors_entire_canvas_and_transforms_physical_pivot() {
        let mut image = RgbaImage::new(8, 4);
        for y in 0..2 {
            for x in 0..2 {
                image.put_pixel(x, y, Rgba([4, 5, 6, 120]));
            }
        }
        let options = settings(8, 6, 3, TileMode::Pad, TileAnchor::BottomCenter);
        let (output, pivot) = render(&image, 2, (3.0, 1.0), &options).unwrap();
        assert_eq!(output.dimensions(), (24, 18));
        assert_eq!(pivot, (10.5, 13.5));
        assert_eq!(*output.get_pixel(6, 12), Rgba([4, 5, 6, 120]));
        assert_eq!(output.get_pixel(5, 12)[3], 0);
        assert_eq!(output.get_pixel(6, 11)[3], 0);
        let (_, center_pivot) = render(
            &image,
            2,
            (3.0, 1.0),
            &settings(8, 6, 3, TileMode::Pad, TileAnchor::Center),
        )
        .unwrap();
        assert_eq!(center_pivot, (10.5, 7.5));
    }

    #[test]
    fn fit_prefers_integer_enlargement_and_nearest_shrink_has_no_new_rgba() {
        let image = RgbaImage::from_fn(4, 2, |x, y| {
            Rgba([x as u8 * 40, y as u8 * 80, 7, if x == 0 { 50 } else { 255 }])
        });
        let options = settings(10, 8, 1, TileMode::Fit, TileAnchor::Center);
        assert_eq!(
            fitted_dimensions(image.dimensions(), &options).unwrap(),
            (8, 4)
        );
        let (output, pivot) = render(&image, 1, (1.0, 0.5), &options).unwrap();
        assert_eq!(pivot, (3.0, 3.0));
        let colors: HashSet<_> = image
            .pixels()
            .map(|pixel| pixel.0)
            .chain(std::iter::once([0; 4]))
            .collect();
        assert!(output.pixels().all(|pixel| colors.contains(&pixel.0)));
        let shrink = settings(3, 3, 2, TileMode::Fit, TileAnchor::BottomCenter);
        assert_eq!(
            fitted_dimensions(image.dimensions(), &shrink).unwrap(),
            (3, 2)
        );
        let (output, pivot) = render(&image, 1, (2.0, 1.0), &shrink).unwrap();
        assert_eq!(pivot, (3.0, 4.0));
        assert!(output.pixels().all(|pixel| colors.contains(&pixel.0)));
    }

    #[test]
    fn padding_rejects_oversize_and_grid_requires_uniform_full_blocks() {
        let mut image = RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 0]));
        image.put_pixel(1, 1, Rgba([10, 20, 30, 0]));
        assert_eq!(logical_image(&image, 2).unwrap().dimensions(), (2, 2));
        image.put_pixel(1, 1, Rgba([10, 20, 30, 1]));
        assert!(logical_image(&image, 2).is_err());
        assert!(logical_image(&RgbaImage::new(3, 4), 2).is_err());
        assert!(render(
            &RgbaImage::new(8, 4),
            1,
            (0.0, 0.0),
            &settings(4, 4, 1, TileMode::Pad, TileAnchor::Center)
        )
        .is_err());
        for options in [
            settings(0, 32, 1, TileMode::Fit, TileAnchor::Center),
            settings(32, 32, 0, TileMode::Fit, TileAnchor::Center),
            settings(u32::MAX, 2, 64, TileMode::Fit, TileAnchor::Center),
            settings(16384, 16384, 1, TileMode::Fit, TileAnchor::Center),
        ] {
            assert!(options.dimensions().is_err());
        }
    }
}
