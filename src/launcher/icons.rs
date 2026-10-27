use super::*;

pub(crate) fn load_builtin_icon(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let mut buffer = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    // PNG decodes as straight RGBA; GPUI consumes straight BGRA.
    for pixel in buffer.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

pub(super) fn builtin_row_icon(
    icon: &Option<Arc<RenderImage>>,
    pixels: u32,
) -> Option<Arc<RenderImage>> {
    let icon = icon.as_ref()?;
    let dimensions = icon.size(0);
    prepare_row_icon(
        native::IconImage {
            width: dimensions.width.0 as u32,
            height: dimensions.height.0 as u32,
            bgra: icon.as_bytes(0)?.to_vec(),
        },
        pixels,
    )
}

pub(super) fn icon_neighborhood(row: usize, count: usize, radius: usize) -> std::ops::Range<usize> {
    row.saturating_sub(radius).min(count)..row.saturating_add(radius).saturating_add(1).min(count)
}

pub(super) fn row_icon_pixels(scale: f32) -> u32 {
    // Exact-size filtering followed by fractional-DPI GPU sampling softened
    // small glyphs. Keep 25% sampling headroom, capped at the shell's 48px
    // source, to preserve detail without retaining oversized rasters.
    (22.0 * scale * 1.25).ceil().clamp(1.0, 48.0) as u32
}

pub(crate) fn prepare_row_icon(icon: native::IconImage, pixels: u32) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
    // The GPU uses bilinear sampling without mipmaps. Prefilter on the worker
    // before upload so minifying a 48px source does not skip fine icon details.
    // Lanczos preserves small edges better than the previous box average, which
    // visibly softened app glyphs at low/fractional DPI.
    // Preserve undersized sources rather than allocating an upscaled bitmap.
    let buffer = if buffer.width() > pixels || buffer.height() > pixels {
        let ratio = pixels as f64 / buffer.width().max(buffer.height()) as f64;
        let width = (buffer.width() as f64 * ratio).round().max(1.0) as u32;
        let height = (buffer.height() as f64 * ratio).round().max(1.0) as u32;
        image::imageops::resize(
            &buffer,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        buffer
    };
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

/// Decode `path`'s icon on the background thread pool, then hand the result
/// back to the catalog and repaint. GDI/COM icon extraction is too slow to do
/// synchronously in the render path (it blocks keystrokes).
pub(super) fn spawn_icon_load(
    path: PathBuf,
    pixel_size: u32,
    catalog: Rc<RefCell<Catalog>>,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    cx.spawn(async move |list, cx| {
        let decode_path = path.clone();
        let rendered = cx
            .background_spawn(async move {
                native::extract_icon_rgba(&decode_path)
                    .and_then(|icon| prepare_row_icon(icon, pixel_size))
            })
            .await;

        catalog
            .borrow_mut()
            .set_icon_ready(path, pixel_size, rendered);
        let _ = list.update(cx, |_, cx| cx.notify());
    })
    .detach();
}
