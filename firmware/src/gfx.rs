use embedded_graphics::{
    fonts::{Font6x8, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Rectangle, RoundedRectangle},
    style::{MonoTextStyleBuilder, PrimitiveStyleBuilder},
};
use shared::message;

const WIDTH: i32 = 128;
const HEIGHT: i32 = 64;
const MARGIN: i32 = 5;

fn draw<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    let text = MonoTextStyleBuilder::new(Font6x8)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off)?;

    Text::new("CPU", Point::new(MARGIN, MARGIN))
        .into_styled(text)
        .draw(display)?;

    double_bar_graph(
        &mut display.translated(Point::new(MARGIN, 14)),
        Size::new((WIDTH - MARGIN * 2) as u32, 10),
        perf.all_cores_load,
        perf.peak_core_load,
    )?;

    Ok(())
}

fn double_bar_graph<T>(
    display: &mut T,
    size: Size,
    low_val: f32,
    high_val: f32,
) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    let outline = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .fill_color(BinaryColor::Off)
        .build();

    let solid = PrimitiveStyleBuilder::new()
        .fill_color(BinaryColor::On)
        .build();

    let height = size.height;
    let width = ((size.width as f32) * high_val) as u32;

    RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(0, 0), Size::new(width, height)),
        Size::new(2, 2),
    )
    .into_styled(outline)
    .draw(display)?;

    let width = ((size.width as f32) * low_val) as u32;

    RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(0, 0), Size::new(width, height)),
        Size::new(2, 2),
    )
    .into_styled(solid)
    .draw(display)?;

    Ok(())
}
