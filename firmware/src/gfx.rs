use embedded_graphics::{
    mono_font::{MonoFont, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::Text,
};
use heapless::String;
use shared::message;

const DISP_WIDTH: i32 = 240;
const DISP_X_PAD: i32 = 3;
const DISP_Y_PAD: i32 = 3;
const FONT: MonoFont = embedded_graphics::mono_font::ascii::FONT_10X20;
const LINE_Y_PAD: i32 = 6;
const BAR_WIDTH: u32 = (DISP_WIDTH - DISP_X_PAD * 2) as u32;
const BAR_HEIGHT: u32 = 15;
const BACKGROUND_COLOR: Rgb565 = Rgb565::BLACK;
const TEXT_COLOR: Rgb565 = Rgb565::WHITE;

struct ColorScheme {
    background: Rgb565,
    cpu_text: Rgb565,
    cpu_bar_avg: Rgb565,
    cpu_bar_peak: Rgb565,
    mem_text: Rgb565,
    mem_bar: Rgb565,
}

const DAY_COLORS: ColorScheme = ColorScheme {
    background: Rgb565::new(24, 48, 24),
    cpu_text: Rgb565::BLACK,
    cpu_bar_avg: Rgb565::new(10, 10, 22),
    cpu_bar_peak: Rgb565::new(15, 30, 28),
    mem_text: Rgb565::BLACK,
    mem_bar: Rgb565::new(7, 43, 11),
};

const NIGHT_COLORS: ColorScheme = ColorScheme {
    background: Rgb565::BLACK,
    cpu_text: Rgb565::new(24, 48, 24),
    cpu_bar_avg: Rgb565::new(10, 10, 22),
    cpu_bar_peak: Rgb565::new(3, 3, 8),
    mem_text: Rgb565::new(24, 48, 24),
    mem_bar: Rgb565::new(0, 30, 3),
};

// Renders a simple text message, for errors, etc.
pub fn draw_message<T>(display: &mut T, msg: &str) -> Result<(), T::Error>
where
    T: DrawTarget<Color = Rgb565>,
{
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(TEXT_COLOR)
        .build();

    // Clear screen and render message.
    display.clear(BACKGROUND_COLOR)?;
    Text::new(msg, text_point(DISP_X_PAD, 1), text_style).draw(display)?;

    Ok(())
}

// Renders the full performance display.
pub fn draw_perf<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<Color = Rgb565>,
{
    let colors = if perf.daytime {
        DAY_COLORS
    } else {
        NIGHT_COLORS
    };

    let cpu_text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(colors.cpu_text)
        .build();

    let mem_text_style = MonoTextStyleBuilder::new()
        .font(&FONT)
        .text_color(colors.mem_text)
        .build();

    let mem_bar_style = PrimitiveStyleBuilder::new()
        .fill_color(colors.mem_bar)
        .build();

    // Clear and begin drawing.
    display.clear(colors.background)?;

    // CPU heading.
    Text::new("CPU", text_point(DISP_X_PAD, 0), cpu_text_style).draw(display)?;

    // Average CPU percent display, right aligned.
    let mut avg = percent_string(perf.all_cores_avg, true);
    avg.push_str("% Avg").unwrap();
    Text::new(
        avg.as_str(),
        text_point_right(0, avg.as_str()),
        cpu_text_style,
    )
    .draw(display)?;

    draw_cpu_bar_graph(display, perf)?;

    // RAM heading.
    Text::new("RAM", text_point(DISP_X_PAD, 2), mem_text_style).draw(display)?;

    // Free memory percent display, right aligned.
    let mut avg = percent_string(1.0 - perf.memory_load, false);
    avg.push_str("% Free").unwrap();
    Text::new(
        avg.as_str(),
        text_point_right(2, avg.as_str()),
        mem_text_style,
    )
    .draw(display)?;

    // Draw memory used bar.
    bar_graph(
        display,
        mem_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(3)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.memory_load,
    )?;

    Ok(())
}

// Renders the overlaid CPU bar graphs, can be used without clearing the screen first.
pub fn draw_cpu_bar_graph<T>(display: &mut T, perf: &message::PerfData) -> Result<(), T::Error>
where
    T: DrawTarget<Color = Rgb565>,
{
    let colors = if perf.daytime {
        DAY_COLORS
    } else {
        NIGHT_COLORS
    };

    let cpu_peak_bar_style = PrimitiveStyleBuilder::new()
        .fill_color(colors.cpu_bar_peak)
        .build();

    let cpu_avg_bar_style = PrimitiveStyleBuilder::new()
        .fill_color(colors.cpu_bar_avg)
        .build();

    // Draw longer peak core load bar.
    bar_graph(
        display,
        cpu_peak_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(1)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.peak_core_load,
    )?;

    // Draw shorter, overlapping all cores load bar.
    bar_graph(
        display,
        cpu_avg_bar_style,
        Point::new(DISP_X_PAD, line_y_offset(1)),
        Size::new(BAR_WIDTH, BAR_HEIGHT),
        perf.all_cores_load,
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
    style: PrimitiveStyle<Rgb565>,
    offset: Point,
    size: Size,
    val: f32,
) -> Result<(), T::Error>
where
    T: DrawTarget<Color = Rgb565>,
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
        (b'0' + d as u8) as char
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
