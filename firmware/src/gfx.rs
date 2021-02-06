use embedded_graphics::{
    fonts::{Font6x12, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    style::{PrimitiveStyle, PrimitiveStyleBuilder, TextStyleBuilder},
};
use heapless::{consts::*, String};
use shared::message;

const DISP_WIDTH: i32 = 128;
const X_PAD: i32 = 1;
const Y_PAD: i32 = 2;
const CHAR_HEIGHT: i32 = 14;
const CHAR_WIDTH: i32 = 6;
const BAR_WIDTH: u32 = (DISP_WIDTH - X_PAD * 2) as u32;

pub fn draw_message<T>(display: &mut T, msg: &str) -> Result<(), T::Error>
where
    T: DrawTarget<BinaryColor>,
{
    let text = TextStyleBuilder::new(Font6x12)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off)?;

    Text::new(msg, Point::new(X_PAD, line_y(1)))
        .into_styled(text)
        .draw(display)?;

    return Ok(());
}

pub fn draw_perf<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<BinaryColor>,
{
    // Invert the display during the day to even out burn-in.
    let background = if perf.daytime {
        BinaryColor::On
    } else {
        BinaryColor::Off
    };
    let foreground = if perf.daytime {
        BinaryColor::Off
    } else {
        BinaryColor::On
    };

    let text = TextStyleBuilder::new(Font6x12)
        .text_color(foreground)
        .build();

    let outline_bar = PrimitiveStyleBuilder::new()
        .stroke_color(foreground)
        .stroke_width(1)
        .fill_color(background)
        .build();

    let solid_bar = PrimitiveStyleBuilder::new().fill_color(foreground).build();

    display.clear(background)?;

    Text::new("CPU", Point::new(X_PAD, line_y(0)))
        .into_styled(text)
        .draw(display)?;

    // Average CPU percent display.
    let mut avg = percent_string(perf.all_cores_avg, true);
    avg.push_str("% Avg").unwrap();
    let avg_width = (avg.len() as i32) * CHAR_WIDTH;
    Text::new(
        avg.as_str(),
        Point::new(DISP_WIDTH - X_PAD - avg_width, line_y(0)),
    )
    .into_styled(text)
    .draw(display)?;

    // Draw longer peak core load bar.
    bar_graph(
        display,
        outline_bar,
        Point::new(X_PAD, line_y(1)),
        Size::new(BAR_WIDTH, 10),
        perf.peak_core_load,
    )?;

    // Draw shorter, overlapping all cores load bar.
    bar_graph(
        display,
        solid_bar,
        Point::new(X_PAD, line_y(1)),
        Size::new(BAR_WIDTH, 10),
        perf.all_cores_load,
    )?;

    Text::new("RAM", Point::new(X_PAD, line_y(2)))
        .into_styled(text)
        .draw(display)?;

    // Free memory percent display.
    let mut avg = percent_string(1.0 - perf.memory_load, false);
    avg.push_str("% Free").unwrap();
    let avg_width = (avg.len() as i32) * CHAR_WIDTH;
    Text::new(
        avg.as_str(),
        Point::new(DISP_WIDTH - X_PAD - avg_width, line_y(2)),
    )
    .into_styled(text)
    .draw(display)?;

    // Draw used memory bar.
    bar_graph(
        display,
        solid_bar,
        Point::new(X_PAD, line_y(3)),
        Size::new(BAR_WIDTH, 10),
        perf.memory_load,
    )?;

    Ok(())
}

fn line_y(line: i32) -> i32 {
    Y_PAD + (line * (Y_PAD + CHAR_HEIGHT))
}

fn bar_graph<T>(
    display: &mut T,
    style: PrimitiveStyle<BinaryColor>,
    offset: Point,
    size: Size,
    val: f32,
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

    // Wide, high value bar.
    Rectangle::new(
        Point::new(0, 0) + offset,
        Point::new(scale_x(val), height) + offset,
    )
    .into_styled(style)
    .draw(display)?;

    Ok(())
}

fn percent_string(ratio: f32, fractional: bool) -> String<U16> {
    fn digit(d: i32) -> char {
        (('0' as u8) + d as u8) as char
    }

    let mut num = ((ratio * 1000.0) as i32).min(999);
    let tenths = num % 10;
    num /= 10;
    let ones = num % 10;
    num /= 10;
    let tens = num % 10;

    let mut result = String::new();
    result
        .push(if tens == 0 { ' ' } else { digit(tens) })
        .unwrap();
    result.push(digit(ones)).unwrap();

    if fractional {
        result.push('.').unwrap();
        result.push(digit(tenths)).unwrap();
    }

    result
}
