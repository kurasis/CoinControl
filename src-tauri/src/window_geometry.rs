//! Fit the decorated window to the monitor's physical work area, including DPI.
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

fn fitted_size(
    work: PhysicalSize<u32>,
    frame: PhysicalSize<u32>,
    desired: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    PhysicalSize::new(
        desired
            .width
            .min(work.width.saturating_sub(frame.width).max(1)),
        desired
            .height
            .min(work.height.saturating_sub(frame.height).max(1)),
    )
}

fn fitted_position(
    start: PhysicalPosition<i32>,
    work: PhysicalSize<u32>,
    outer: PhysicalSize<u32>,
    position: PhysicalPosition<i32>,
) -> PhysicalPosition<i32> {
    let clamp = |origin: i32, available: u32, size: u32, value: i32| {
        let end = i64::from(origin) + i64::from(available.saturating_sub(size));
        i64::from(value)
            .clamp(i64::from(origin), end)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    };
    PhysicalPosition::new(
        clamp(start.x, work.width, outer.width, position.x),
        clamp(start.y, work.height, outer.height, position.y),
    )
}

fn fit<R: Runtime>(window: &WebviewWindow<R>, update_minimum: bool) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let inner = window.inner_size()?;
    let outer = window.outer_size()?;
    let frame = PhysicalSize::new(
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    if update_minimum {
        let minimum = LogicalSize::new(1024.0, 720.0).to_physical::<u32>(window.scale_factor()?);
        window.set_min_size(Some(fitted_size(area.size, frame, minimum)))?;
    }
    let size = fitted_size(area.size, frame, inner);
    if size != inner {
        window.set_size(size)?;
    }
    // Read back the actual frame after resizing; taskbars and negative monitor origins matter.
    let position = window.outer_position()?;
    let adjusted = fitted_position(area.position, area.size, window.outer_size()?, position);
    if adjusted != position {
        window.set_position(adjusted)?;
    }
    Ok(())
}

pub fn fit_main<R: Runtime>(app: &impl Manager<R>, update_minimum: bool) {
    if let Some(window) = app.get_webview_window("main")
        && let Err(error) = fit(&window, update_minimum)
    {
        tracing::warn!(%error, "could not fit window to monitor work area");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_dpi_minimum_leaves_space_for_caption_and_taskbar() {
        let minimum = LogicalSize::new(1024.0, 720.0).to_physical::<u32>(2.0);
        let fitted = fitted_size(
            PhysicalSize::new(1920, 1040),
            PhysicalSize::new(16, 78),
            minimum,
        );
        assert_eq!(fitted, PhysicalSize::new(1904, 962));
        assert!(fitted.width + 16 <= 1920 && fitted.height + 78 <= 1040);
    }

    #[test]
    fn preserves_valid_position_and_clamps_negative_monitor_bounds() {
        let origin = PhysicalPosition::new(-1920, -100);
        let work = PhysicalSize::new(1920, 1040);
        let size = PhysicalSize::new(1200, 800);
        let valid = PhysicalPosition::new(-1800, 0);
        assert_eq!(fitted_position(origin, work, size, valid), valid);
        assert_eq!(
            fitted_position(origin, work, size, PhysicalPosition::new(0, -200)),
            PhysicalPosition::new(-1200, -100)
        );
    }
}
