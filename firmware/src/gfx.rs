use embedded_graphics::{
    fonts::{Font6x8, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    style::{PrimitiveStyleBuilder, TextStyleBuilder},
};
use shared::message;

const WIDTH: i32 = 128;
const MARGIN: i32 = 5;

pub fn draw<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<BinaryColor>,
{
    let text = TextStyleBuilder::new(Font6x8)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off)?;

    Text::new("CPU", Point::new(MARGIN, MARGIN))
        .into_styled(text)
        .draw(display)?;

    double_bar_graph(
        display,
        Point::new(MARGIN, 14),
        Size::new((WIDTH - MARGIN * 2) as u32, 10),
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
    let outline = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .fill_color(BinaryColor::Off)
        .build();

    let solid = PrimitiveStyleBuilder::new()
        .fill_color(BinaryColor::On)
        .build();

    let height = size.height as i32;
    let width = ((size.width as f32) * high_val) as i32;

    Rectangle::new(
        Point::new(0, 0) + offset,
        Point::new(width, height) + offset,
    )
    .into_styled(outline)
    .draw(display)?;

    let width = ((size.width as f32) * low_val) as i32;

    Rectangle::new(
        Point::new(0, 0) + offset,
        Point::new(width, height) + offset,
    )
    .into_styled(solid)
    .draw(display)?;

    Ok(())
}
