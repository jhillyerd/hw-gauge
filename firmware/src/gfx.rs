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
const BAR_HEIGHT: u32 = 10;

// Renders a simple text message, for errors, etc.
pub fn draw_message<T>(display: &mut T, msg: &str) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(BinaryColor::On)
        .build();

    // Clear screen and render message.
    display.clear(BinaryColor::Off)?;
    Text::new(msg, text_point(DISP_X_PAD, 1), text_style).draw(display)?;

    return Ok(());
}

// Renders the full performance display.
pub fn draw_perf<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<Color = BinaryColor>,
{
    // Invert the display during the day to even out burn-in.
    let (foreground, background) = if perf.daytime {
        (BinaryColor::Off, BinaryColor::On)
    } else {
        (BinaryColor::On, BinaryColor::Off)
    };

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(foreground)
        .build();

    let outline_bar_style = PrimitiveStyleBuilder::new()
        .stroke_color(foreground)
        .stroke_width(1)
        .fill_color(background)
        .build();

    let solid_bar_style = PrimitiveStyleBuilder::new().fill_color(foreground).build();

    // Clear and begin drawing.
    display.clear(background)?;

    // CPU heading.
    Text::new("CPU", text_point(DISP_X_PAD, 0), text_style).draw(display)?;

    // Average CPU percent display, right aligned.
    let mut avg = percent_string(perf.all_cores_avg, true);
    avg.push_str("% Avg").unwrap();
    Text::new(
        avg.as_str(),
        text_point_right(0, avg.as_str()),
        text_style,
    )
    .draw(display)?;

    // Draw longer peak core load bar.
    bar_graph(
        display,
        outline_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(1)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.peak_core_load,
    )?;

    // Draw shorter, overlapping all cores load bar.
    bar_graph(
        display,
        solid_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(1)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.all_cores_load,
    )?;

    // RAM heading.
    Text::new("RAM", text_point(DISP_X_PAD, 2), text_style).draw(display)?;

    // Free memory percent display, right aligned.
    let mut avg = percent_string(1.0 - perf.memory_load, false);
    avg.push_str("% Free").unwrap();
    Text::new(
        avg.as_str(),
        text_point_right(2, avg.as_str()),
        text_style,
    )
    .draw(display)?;

    // Draw memory used bar.
    bar_graph(
        display,
        solid_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(3)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.memory_load,
    )?;

    Ok(())
}

// Returns the screen Y pixel offset for the top of the specified text line number.
fn line_y_offset(line: i32) -> i32 {
    DISP_Y_PAD + (line * (LINE_Y_PAD + FONT.character_size.height as i32))
}

// Returns the point to render text for the specified X pixel offset and line number.
fn text_point(x: i32, line: i32) -> Point {
    Point::new(x, line_y_offset(line) + FONT.baseline as i32)
}

// Returns the point to render right-aligned text for the specified line number.
fn text_point_right(line: i32, text: &str) -> Point {
    let text_width = text.len() as i32 * FONT.character_size.width as i32;
    text_point(DISP_WIDTH - DISP_X_PAD - text_width, line)
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
