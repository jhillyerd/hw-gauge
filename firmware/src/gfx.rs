use embedded_graphics::{
    fonts::{Font6x12, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    style::{PrimitiveStyleBuilder, TextStyleBuilder},
};
use shared::message;

const DISP_WIDTH: i32 = 128;
const X_PAD: i32 = 0;
const Y_PAD: i32 = 2;
const LINE_HEIGHT: i32 = 12;

pub fn draw<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<BinaryColor>,
{
    let text = TextStyleBuilder::new(Font6x12)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off)?;

    Text::new("CPU", Point::new(X_PAD, Y_PAD))
        .into_styled(text)
        .draw(display)?;

    double_bar_graph(
        display,
        Point::new(X_PAD, Y_PAD * 2 + LINE_HEIGHT),
        Size::new((DISP_WIDTH - X_PAD * 2) as u32, 10),
        perf.all_cores_load,
        perf.peak_core_load,
    )?;

    Ok(())
}

fn double_bar_graph<T>(
    display: &mut T,
    offset: Point,
    size: Size,
    low_val: f32,
    high_val: f32,
) -> Result<(), T::Error>
where
    T: DrawTarget<BinaryColor>,
{
    let height = size.height as i32;
    let max_x = (size.width - 1) as i32;
    let max_x_f = max_x as f32;
    let scale_x = |val: f32| {
        let x = (max_x_f * val) as i32;
        x.min(max_x)
    };

    let outline = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .fill_color(BinaryColor::Off)
        .build();

    let solid = PrimitiveStyleBuilder::new()
        .fill_color(BinaryColor::On)
        .build();

    // Wide, high value bar.
    Rectangle::new(
        Point::new(0, 0) + offset,
        Point::new(scale_x(high_val), height) + offset,
    )
    .into_styled(outline)
    .draw(display)?;

    // Narrow, low value bar.
    Rectangle::new(
        Point::new(0, 0) + offset,
        Point::new(scale_x(low_val), height) + offset,
    )
    .into_styled(solid)
    .draw(display)?;

    Ok(())
}
