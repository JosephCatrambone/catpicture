extern crate image;

/* catpicture
 * @author Joseph Catrambone <jo.jcat@gmail.com>
 * Release notes:
 * v0.1.0 : First release -- Supports just '#' for output style.  Allows -c for full-color mode, -r, -w, -h to change sizes.
 * v0.2.0 : Automatically select correct aspect ratio when only -w or -h supplied.  Support force-grey.
 * v0.3.0 : Add new line algorithms with --line.  Can fill BG instead of '#', supports BG, '#', and gradient.
 * v0.4.0 : Use nearest neighbor to select the best looking ascii stand-in.
 **v0.5.0 : Hardening and improvements to robustness.  Bounds checking.  Ready for beta release.
 * v0.6.0 : Allow threshold to be set for _not_ drawing, so if people want black text to show as empty space (for writing to text file), that can be done.
 * v0.7.0 : Introduce FFT to split high-frequency pixels from low frequency pixels. Draw high frequency in FG with font, low frequency in BG.
 * v1.0.0 : Ready for release.
 */

use std::char;
use std::clone::Clone;
use std::collections::HashMap;
use std::fmt::Write;
use std::env;
use std::io::{Cursor, Read, self};
use std::option::Option;
use std::path::Path;

use image::{GenericImage, imageops, FilterType, DynamicImage, Pixel}; // Pixel used for .to_luma.

const COMPARISON_SET : &'static str = "characters.png";
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
catpicture [--help|-?] [-c] [-w] [-h] [-r x1 y1 x2 y2] [-g] [-d block|art|char x] [filename]
--help/-?	This message.
-c	Try to use full color instead of nearest XTERM color. 
-w	Set output width.
-h	Set output height.
-r xywh	Given four points (left top right bottom), cut the specified region from the picture for display.
-g	Force greyscale on image.
-d	Specify the 'draw mode' for the output. 
		block -> Only background will be filled.
		art -> Use nearest neighbor to find the best approximate character match for a patch.
		char -> Use the specified character to draw.
filename	The name of the image to open.  If unspecified, reads from stdin.
"#;

#[derive(PartialEq)]
enum DrawMode {
	Block,
	Char(char),
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
		draw_mode : DrawMode::Block,
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
		} else if arg == OUTPUT_HEIGHT { 
			// Check if the user is passing -h to try and get to help.
			if i+1 >= args.len() {
				println!("-h specifies the height, but that argument is missing. You probably meant to use -? or --help");
				settings.show_help = true;
				continue;
			} else {
				settings.output_height = Some(args[i+1].parse::<u32>().unwrap());
				skip_args = 1;
			}
		} else if arg == LINE_ALGORITHM {
			skip_args = 0; // Set this inside the switch.
			let mode = &args[i+1].to_lowercase();
			settings.draw_mode = match mode.as_ref() {
				"block" => DrawMode::Block,
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
	// let mut out : String = String::new();
	// TODO: write!(&mut res, "{}", c).unwrap()
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
		(None, None) => (DEFAULT_WIDTH, (DEFAULT_WIDTH as f32/aspect_ratio) as u32),
	};
	(target_width, target_height)
}

fn build_character_image_vector(font_image : &DynamicImage) -> Vec<DynamicImage> {
	let num_characters : u32 = (b'~' - b' ') as u32;
	let mut characters = Vec::with_capacity(num_characters as usize);
	let character_width = font_image.dimensions().0 / num_characters;
	let character_height = font_image.dimensions().1;
	for i in 0..num_characters {
		let mut font_image_copy = font_image.clone();
		let source_character = font_image_copy.crop(i*character_width, 0, character_width, character_height);
		characters.push(source_character);
	}
	characters
}

fn find_best_character(x : u32, y : u32, w : u32, h : u32, input_image : &DynamicImage, characters : &Vec<DynamicImage>) -> char {
	// This takes the 'font image', which is a list of all the printable ascii characters in a single row left to right,
	// starting at the lowest printable one, ' ' and ending at '~'.  
	// w and h are the final, desired size of the input_image.  x and y are the pixel that will be printed in the final image.
	// x and y are the location in the resized pixel, so they'll have to be multiplied by the pixel_width and height to get correct values.
	let (input_width, input_height) = input_image.dimensions();
	let pixel_width = input_width/w; // Not really 'pixel width', but the sample width that lets us feed into the image.
	let pixel_height = input_height/h;
	let (character_width, character_height) = characters[0].dimensions();
	let mut best_char = ' ';
	let mut best_distance : u32 = character_width*character_height*255+1; // Max distance.
	let input_image_region = input_image.clone().crop(x*pixel_width, y*pixel_width, pixel_width, pixel_height);
	let target_region = imageops::resize(&input_image_region, character_width, character_height, FilterType::CatmullRom);
	'charloop: for (char_index, source_character) in characters.iter().enumerate() {
		// Calculate distance between this character and the one in question.
		let mut distance : u32 = 0;
		//for (candidate_pixel, target_pixel) in target_region.pixels().zip(source_character.pixels()) {
		for py in 0..character_height {
			for px in 0..character_width {
				let target_pixel = target_region.get_pixel(px, py);
				let candidate_pixel = source_character.get_pixel(px, py);
				distance += (candidate_pixel.to_luma().data[0] as i32 - target_pixel.to_luma().data[0] as i32).abs() as u32;
			}
			// We are only breaking on the outermost loop because we don't want to incur the branching cost on the inner loop.
			// If the distance is already greater than the best character, don't keep comparing.
			if distance > best_distance { 
				continue 'charloop;
			}
		} 
		if distance < best_distance {
			best_distance = distance;
			best_char = char::from_u32(char_index as u32 + b' ' as u32).unwrap();
		}
	}
	best_char
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

		// Since we're calling this every pixel, let's preload the comparison NN set for the 'best character' search, but only if the mode is 'Art'.
		// TODO: Make this optionally loaded.
		let font_image = image::load(Cursor::new(&include_bytes!("characters.png")[..]), image::PNG).unwrap(); // TODO: MAGIC NUMBER - Make 'characters' a magic number.
		let character_image_vector = build_character_image_vector(&font_image);

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
				DrawMode::Block => { print_color_character(' ', (0, 0, 0), rgb, settings.use_full_colors) },
				DrawMode::Char(c) => { print_color_character(c, rgb, (0, 0, 0), settings.use_full_colors) },
				DrawMode::Art => { print_color_character(find_best_character(x, y, target_width, target_height, &img, &character_image_vector), rgb, (0, 0, 0), settings.use_full_colors) },
			};

			// Generate newline if we're at the edge of the output.
			if x == target_width-1 {
				print!("\n");
			}
		}
	}
}
