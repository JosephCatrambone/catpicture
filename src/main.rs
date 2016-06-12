extern crate image;

/* catpicture
 * @author Joseph Catrambone <jo.jcat@gmail.com>
 * Release notes:
 * v0.1.0 : First release -- Supports just '#' for output style.  Allows -c for full-color mode, -r, -w, -h to change sizes.
 * v0.2.0 : Automatically select correct aspect ratio when only -w or -h supplied.  Support force-grey.
 **v0.3.0 : Add new line algorithms with --line.  Can fill BG instead of '#', supports BG, '#', and gradient.
 * v0.4.0 : Use nearest neighbor to select the best looking ascii stand-in.
 * v0.5.0 : Hardening and improvements to robustness.  Bounds checking.  Ready for beta release.
 * v0.6.0 : Allow threshold to be set for _not_ drawing, so if people want black text to show as empty space (for writing to text file), that can be done.
 * v1.0.0 : Ready for release.
 */

use std::collections::HashMap;
use std::f64; // For atan2.
use std::env;
use std::io::{Read, self};
use std::option::Option;
use std::path::Path;

use image::{GenericImage, imageops, FilterType, DynamicImage, Pixel}; // Pixel used for .to_luma.

const DEFAULT_WIDTH : u32 = 80;
const LINE_ALGORITHM : &'static str = "-d";
const USE_FULL_COLORS : &'static str = "-c";
const OUTPUT_WIDTH : &'static str = "-w";
const OUTPUT_HEIGHT : &'static str = "-h";
const SOURCE_RECT : &'static str = "-r";
const FORCE_GREY : &'static str = "-g";
const HELP_SHORT : &'static str = "-?";
const HELP_LONG : &'static str = "--help";
const HELP_STRING : &'static str = r#"
Usage: 
catpicture [--help|-?] [-c] [-w] [-h] [-r x1 y1 x2 y2] [-g] [-d none|block|line|art|char x] [filename]
--help/-?	This message.
-c	Try to use full color instead of nearest XTERM color. 
-w	Set output width.
-h	Set output height.
-r xywh	Given four points (left top right bottom), cut the specified region from the picture for display.
-g	Force greyscale on image.
-d	Specify the 'draw mode' for the output. 
		none -> Only background color will be filled.
		block -> A single '#' will be used on top of a black background.
		line -> Find the steepest gradient in the image and fill with an appropriate ascii character.
		art -> Use nearest neighbor to find the best approximate character match for a patch.
		char -> Use the specified character to draw.
filename	The name of the image to open.  If unspecified, reads from stdin.
"#;

enum DrawMode {
	None,
	Char(char),
	Line,
	Art,
}

struct Settings {
	input_filename : String, // Will be "" for stdin.
	output_width : Option<u32>,
	output_height : Option<u32>,
	region : Option<(u32, u32, u32, u32)>,
	use_full_colors : bool,
	show_help : bool,
	force_grey : bool,
	draw_mode : DrawMode,
}

fn parse_args(args : Vec<String>) -> Settings {
	let mut settings = Settings {
		input_filename : "".to_string(),
		output_width : None,
		output_height : None,
		region : None,
		show_help : false,
		use_full_colors : false,
		force_grey : false,
		draw_mode : DrawMode::Char('#'),
	};

	let mut skip_args = 0; // True if the argument was consumed.
	for i in 1..args.len() {
		if skip_args > 0 { // We consumed this argument as part of the first run.
			skip_args -= 1;
			continue;
		}
		// args[0] == file name.
		let arg = args[i].to_lowercase();
		if arg == USE_FULL_COLORS {
			settings.use_full_colors = true;
		} else if arg == HELP_SHORT || args[i] == HELP_LONG {
			settings.show_help = true;
		} else if arg == OUTPUT_WIDTH { // TODO: Check OOB.
			settings.output_width = Some(args[i+1].parse::<u32>().unwrap());
			skip_args = 1;
		} else if arg == OUTPUT_HEIGHT { // TODO: Check OOB and, if the user has no i+1, display help.
			settings.output_height = Some(args[i+1].parse::<u32>().unwrap());
			skip_args = 1;
		} else if arg == LINE_ALGORITHM {
			skip_args = 0; // Set this inside the switch.
			let mode = &args[i+1].to_lowercase();
			settings.draw_mode = match mode.as_ref() {
				"none" => DrawMode::None,
				"block" => DrawMode::Char('#'),
				"line" => DrawMode::Line,
				"art" => DrawMode::Art,
				"char" => {
					skip_args = 1;
					DrawMode::Char(args[i+2].chars().nth(0).unwrap())
				},
				_ => {
					println!("Unrecognized draw mode.  Defaulting to block.");
					DrawMode::Char('#')
				}
			};
			skip_args += 1; // NOTE: Add one because we may skip another line if we have to get the character.
		} else if arg == SOURCE_RECT {
			settings.region = Some((
				args[i+1].parse::<u32>().unwrap(),
				args[i+2].parse::<u32>().unwrap(),
				args[i+3].parse::<u32>().unwrap(),
				args[i+4].parse::<u32>().unwrap(),
			));
			skip_args = 4;
		} else if arg == FORCE_GREY {
			settings.force_grey = true;
		} else {
			if settings.input_filename == "" && args[i].chars().nth(0).unwrap_or('-') != '-' {
				settings.input_filename = args[i].to_string();
			} else {
				panic!("Unrecognized argument #{}: {}", i, args[i]);
			}
		}
	}

	settings
}

fn print_color_character(c : char, fg : (u8, u8, u8), bg : (u8, u8, u8), use_full_colors : bool) {
	if use_full_colors { // Generate color code.
		// ESC[38;2;<r>;<g>;<b>m (Foreground)
		// ESC[48;2;<r>;<g>;<b>m (Background)
		print!("\u{1B}[38;2;{};{};{}m{}", fg.0, fg.1, fg.2, c);
	} else {
		// If we support full color switching, use that, otherwise, get the nearest color match.
		let mut color_lookup = HashMap::new();
		color_lookup.insert([0u8, 0, 0], 30); // Black.
		color_lookup.insert([255u8, 0, 0], 31); // Red
		color_lookup.insert([0u8, 255, 0], 32); // Green.
		color_lookup.insert([0u8, 255, 255], 33); // Yellow.
		color_lookup.insert([0u8, 0, 255], 34); // Blue
		color_lookup.insert([255u8, 0, 255], 35); // Magenta.
		color_lookup.insert([255u8, 255, 0], 36); // Cyan.
		color_lookup.insert([255u8, 255, 255], 37); // White.

		let mut nearest_foreground_color = 39;
		let mut nearest_foreground_dist = 195075 as i32 + 1; // Past max rgb^2.
		let mut nearest_background_color = 39;
		let mut nearest_background_dist = 194075 as i32 + 1;
		for (color_array, color_code) in &color_lookup {
			let dr = fg.0 as i32 - color_array[0] as i32;
			let dg = fg.1 as i32 - color_array[1] as i32;
			let db = fg.2 as i32 - color_array[2] as i32;
			let dist = dr*dr + dg*dg + db*db;
			if dist < nearest_foreground_dist {
				nearest_foreground_color = *color_code;
				nearest_foreground_dist = dist;
			}
			let dr = bg.0 as i32 - color_array[0] as i32;
			let dg = bg.1 as i32 - color_array[1] as i32;
			let db = bg.2 as i32 - color_array[2] as i32;
			let dist = dr*dr + dg*dg + db*db;
			if dist < nearest_background_dist {
				nearest_background_color = (*color_code) + 10; // Offset by 10 for BG colors.
				nearest_background_dist = dist;
			}
		}
		print!("\u{1B}[{}m\u{1B}[{}m{}", nearest_foreground_color, nearest_background_color, c);
	}
	//print!("\u{1B}[39m"); // Alternate reset.
	print!("\u{1B}[0m"); // Reset
}

fn print_help() {
	println!("{}", HELP_STRING);
}

fn calculate_target_dimension(maybe_width : Option<u32>, maybe_height : Option<u32>, image_width : u32, image_height : u32) -> (u32, u32) {
	let aspect_ratio = image_width as f32 / image_height as f32;
	let (target_width, target_height) = match (maybe_width, maybe_height) { 
		(Some(w), Some(h)) => (w, h),
		(Some(w), None) => (w, (w as f32/aspect_ratio) as u32),
		(None, Some(h)) => ((h as f32*aspect_ratio) as u32, h), 
		(None, None) => (DEFAULT_WIDTH, (DEFAULT_WIDTH as f32*aspect_ratio) as u32),
	};
	(target_width, target_height)
}

/// find_best_line
/// Given a dynamic image, a source pixel, and the width and the height of the output region for the source pixel, find the best fitting line.
fn find_best_line(x : u32, y : u32, w : u32, h : u32, img : &DynamicImage) -> char {
	if x+w > img.dimensions().0 || y+h > img.dimensions().1 {
		return ' ';
	}

	let mut x_grad : f64 = 0.0;
	let mut y_grad : f64 = 0.0;
	let mut cumulative_illumination : f64 = 0.0;
	for py in y..(y+h-1) {
		for px in x..(x+w-1) {
			let p = img.get_pixel(px, py).to_luma().data[0] as i32;
			let dx = p-(img.get_pixel(px+1, py).to_luma().data[0] as i32);
			let dy = p-(img.get_pixel(px, py+1).to_luma().data[0] as i32);
			cumulative_illumination += p as f64;
			x_grad += dx.abs() as f64;
			y_grad += dy.abs() as f64;
		}
	}
	//let pi = f64::consts::PI;
	//let angle = y_grad.atan2(x_grad) * 180.0 / pi;
	let t = 10.0; // Edge threshold.
	if x_grad.abs() < t && y_grad.abs() < t && (cumulative_illumination / (w as f64 * h as f64)) < 0.5 {
		'.'
	} else if x_grad.abs() > y_grad.abs() {
		'|'
	} else if x_grad.abs() < y_grad.abs() {
		'-'
	} else if (x_grad - y_grad).abs() < t && x_grad.abs() > t {
		'+'
	} else {
		'#'
	}
}

fn find_best_character(x : u32, y : u32, w : u32, h : u32, input_image : &DynamicImage) -> char {
	'#'
}

fn main() {
	let arguments: Vec<_> = env::args().collect();
	let settings = parse_args(arguments);

	if settings.show_help {
		print_help();
	} else {
		let mut img = if settings.input_filename == "" { 
			// Don't do this because it expects a UTF-8 string:
			//let mut buffer = String::new();
			//io::stdin().read_to_string(&mut buffer);
			// This may be an option:
			//image::load(std::io::BufReader::new(std::io::stdin()))
			let mut buffer = Vec::<u8>::new();
			match io::stdin().read_to_end(&mut buffer) { _ => () };
			match image::load_from_memory(&buffer) {
				Ok(img) => img,
				Err(problem) => { panic!("Problem loading image from stream: {}", problem); }
			}
		} else { 
			image::open(&Path::new(&settings.input_filename)).unwrap() 
		};

		// Calculate aspect ratio and see if there are any requests outside the image range.
		let (image_width, image_height) = img.dimensions();
		//let color = img.color();
		let (target_width, target_height) = calculate_target_dimension(settings.output_width, settings.output_height, image_width, image_height);
		
		// Only crop if the rect flag is set.
		img = match settings.region {
			Some(rect) => { img.crop(rect.0, rect.1, rect.2-rect.0, rect.3-rect.1) },
			None => { img },
		};
		let target_region = imageops::resize(&img, target_width, target_height, FilterType::CatmullRom); // Nearest/Triangle/CatmullRom/Gaussian/Lanczos3
		//for pixel in target_region.pixels() {
		for (x, y, pixel) in target_region.enumerate_pixels() { // TODO: pixel should be yielding x, y, pixel.
			// Extract pixel color and, if needed, convert it to grey before passing it off to the draw method.
			let mut rgb = (pixel.data[0], pixel.data[1], pixel.data[2]);
			if settings.force_grey {
				// TODO: Check if already luma and use to_luma.
				let sum_rgb : u8 = ((pixel.data[0] as u32 + pixel.data[1] as u32 + pixel.data[2] as u32) / 3) as u8;
				rgb = (sum_rgb, sum_rgb, sum_rgb);
			}

			// Dispatch draw call.  Sometimes we have to select the best character. 
			match settings.draw_mode {
				DrawMode::None => { print_color_character(' ', (0, 0, 0), rgb, settings.use_full_colors) },
				DrawMode::Char(c) => { print_color_character(c, rgb, (0, 0, 0), settings.use_full_colors) },
				DrawMode::Line => { print_color_character(find_best_line(x, y, 5, 5, &img), rgb, (0, 0, 0), settings.use_full_colors) }, // TODO: Change window to correct size.
				DrawMode::Art => { print_color_character(find_best_character(x, y, 5, 5, &img), rgb, (0, 0, 0), settings.use_full_colors) },
			};

			// Generate newline if we're at the edge of the output.
			if x == target_width-1 {
				print!("\n");
			}
		}
	}
}
