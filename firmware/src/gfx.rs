use embedded_graphics::{
    mono_font::{MonoFont, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::Text,
};
use heapless::String;
use shared::message;

const DISP_WIDTH: i32 = 128;
const DISP_X_PAD: i32 = 1;
const DISP_Y_PAD: i32 = 1;
const FONT: MonoFont = embedded_graphics::mono_font::ascii::FONT_6X13;
const LINE_Y_PAD: i32 = 4;
const BAR_WIDTH: u32 = (DISP_WIDTH - DISP_X_PAD * 2) as u32;

pub fn draw_message<T>(display: &mut T, msg: &str) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off)?;

    Text::new(msg, Point::new(DISP_X_PAD, line_y(1)), text_style).draw(display)?;

    return Ok(());
}

pub fn draw_perf<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
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

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(foreground)
        .build();

    let outline_bar = PrimitiveStyleBuilder::new()
        .stroke_color(foreground)
        .stroke_width(1)
        .fill_color(background)
        .build();

    let solid_bar = PrimitiveStyleBuilder::new().fill_color(foreground).build();

    display.clear(background)?;

    Text::new("CPU", text_point(DISP_X_PAD, 0), text_style).draw(display)?;

    // Average CPU percent display, right aligned.
    let mut avg = percent_string(perf.all_cores_avg, true);
    avg.push_str("% Avg").unwrap();
    let avg_width = avg.len() as i32 * FONT.character_size.width as i32;
    Text::new(
        avg.as_str(),
        text_point(DISP_WIDTH - DISP_X_PAD - avg_width, 0),
        text_style,
    )
    .draw(display)?;

    // Draw longer peak core load bar.
    bar_graph(
        display,
        outline_bar,
        Point::new(DISP_X_PAD, line_y(1)),
        Size::new(BAR_WIDTH, 10),
        perf.peak_core_load,
    )?;

    // Draw shorter, overlapping all cores load bar.
    bar_graph(
        display,
        solid_bar,
        Point::new(DISP_X_PAD, line_y(1)),
        Size::new(BAR_WIDTH, 10),
        perf.all_cores_load,
    )?;

    Text::new("RAM", text_point(DISP_X_PAD, 2), text_style).draw(display)?;

    // Free memory percent display, right aligned.
    let mut avg = percent_string(1.0 - perf.memory_load, false);
    avg.push_str("% Free").unwrap();
    let avg_width = avg.len() as i32 * FONT.character_size.width as i32;
    Text::new(
        avg.as_str(),
        text_point(DISP_WIDTH - DISP_X_PAD - avg_width, 2),
        text_style,
    )
    .draw(display)?;

    // Draw used memory bar.
    bar_graph(
        display,
        solid_bar,
        Point::new(DISP_X_PAD, line_y(3)),
        Size::new(BAR_WIDTH, 10),
        perf.memory_load,
    )?;

    Ok(())
}

// Returns the screen Y pixel offset for the top of the specified text line number.
fn line_y(line: i32) -> i32 {
    DISP_Y_PAD + (line * (LINE_Y_PAD + FONT.character_size.height as i32))
}

// Returns the point to render text for the specified X pixel offset and line number.
fn text_point(x: i32, line: i32) -> Point {
    Point::new(x, line_y(line) + FONT.baseline as i32)
}

fn bar_graph<T>(
    display: &mut T,
    style: PrimitiveStyle<BinaryColor>,
    offset: Point,
    size: Size,
    val: f32,
) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    let max_x = size.width - 1;
    let max_x_f = max_x as f32;
    let scale_x = |val: f32| {
        let x = (max_x_f * val) as u32;
        x.min(max_x)
    };

    // Wide, high value bar.
    Rectangle::new(
        Point::new(0, 0) + offset,
        Size::new(scale_x(val), size.height),
    )
    .into_styled(style)
    .draw(display)?;

    Ok(())
}

fn percent_string(ratio: f32, fractional: bool) -> String<16> {
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
