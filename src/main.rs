extern crate image;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{Read, self};
use std::option::Option;
use std::path::Path;

use image::{GenericImage, DynamicImage, imageops, FilterType};

const DEFAULT_WIDTH : u32 = 80;
const DEFAULT_HEIGHT : u32 = 30;
const USE_XTERM_COLORS : &'static str = "-t";
const OUTPUT_WIDTH : &'static str = "-w";
const OUTPUT_HEIGHT : &'static str = "-h";
const SOURCE_RECT : &'static str = "-r";
const FORCE_GREY : &'static str = "-g";
const HELP_SHORT : &'static str = "-?";
const HELP_LONG : &'static str = "--help";
const HELP_STRING : &'static str = r#"
Usage: 
catpicture [--help/-?] [-t] [-w] [-h] [-r x1 y1 x2 y2] [-g] [filename]
--help/-?	This message.
-t	Force XTERM color escapes, matching nearest color. 
-w	Set output width.
-h	Set output height.
-r xywh	Given four points (left top right bottom), cut the specified region from the picture for display.
-g	Force greyscale on image.
filename	The name of the image to open.  If unspecified, reads from stdin.
"#;

struct Settings {
	input_filename : String, // Will be "" for stdin.
	output_width : u32,
	output_height : u32,
	region : Option<(u32, u32, u32, u32)>,
	use_xterm_colors : bool,
	show_help : bool,
	force_grey : bool,
}

fn parse_args(args : Vec<String>) -> Settings {
	let mut settings = Settings {
		input_filename : "".to_string(),
		output_width : DEFAULT_WIDTH,
		output_height : DEFAULT_HEIGHT,
		region : None,
		show_help : false,
		use_xterm_colors : false,
		force_grey : false,
	};

	let mut skip_args = 0; // True if the argument was consumed.
	for i in 1..args.len() {
		if skip_args > 0 { // We consumed this argument as part of the first run.
			skip_args -= 1;
			continue;
		}
		// args[0] == file name.
		if args[i] == USE_XTERM_COLORS {
			settings.use_xterm_colors = true;
		} else if args[i] == HELP_SHORT || args[i] == HELP_LONG {
			settings.show_help = true;
		} else if args[i] == OUTPUT_WIDTH { // TODO: Check OOB.
			settings.output_width = args[i+1].parse::<u32>().unwrap();
			skip_args = 1;
		} else if args[i] == OUTPUT_HEIGHT { // TODO: Check OOB and, if the user has no i+1, display help.
			settings.output_height = args[i+1].parse::<u32>().unwrap();
			skip_args = 1;
		} else if args[i] == SOURCE_RECT {
			settings.region = Some((
				args[i+1].parse::<u32>().unwrap(),
				args[i+2].parse::<u32>().unwrap(),
				args[i+3].parse::<u32>().unwrap(),
				args[i+4].parse::<u32>().unwrap(),
			));
			skip_args = 4;
		} else if args[i] == FORCE_GREY {
			settings.force_grey = true;
		} else if i == args.len()-1 { // If it is the last argument and it's not recognized OR consumed
			settings.input_filename = args[i].to_string();
		} else {
			panic!("Unrecognized argument #{}: {}", i, args[i]);
		}
	}

	settings
}

fn print_color_character(c : char, r : u8, g : u8, b : u8, restrict_to_xterm : bool) {
	if restrict_to_xterm {
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

		let mut nearest_color = 39;
		let mut nearest_dist = 195075 as i32 + 1; // Past max rgb^2.
		for (color_array, color_code) in &color_lookup {
			let dr = r as i32 - color_array[0] as i32;
			let dg = g as i32 - color_array[1] as i32;
			let db = b as i32 - color_array[2] as i32;
			let dist = dr*dr + dg*dg + db*db;
			if dist < nearest_dist {
				nearest_color = *color_code;
				nearest_dist = dist;
			}
		}
		print!("\u{1B}[{}m{}", nearest_color, c);
	} else { // Generate color code.
		// ESC[38;2;<r>;<g>;<b>m (Foreground)
		// ESC[48;2;<r>;<g>;<b>m (Background)
		print!("\u{1B}[38;2;{};{};{}m{}", r, g, b, c);
	}
	//print!("\u{1B}[39m"); // Alternate reset.
	print!("\u{1B}[0m"); // Reset
}

fn print_help() {
	println!("{}", HELP_STRING);
}

fn main() {
	let arguments: Vec<_> = env::args().collect();
	let settings = parse_args(arguments);

	if settings.show_help {
		print_help();
	} else {
		let target_width = settings.output_width;
		let target_height = settings.output_height;
		let mut img = if settings.input_filename == "" { 
			//let mut buffer = String::new();
			//io::stdin().read_to_string(&mut buffer);
			let mut buffer = Vec::<u8>::new();
			io::stdin().read_to_end(&mut buffer);
			match image::load_from_memory(&buffer) {
				Ok(img) => img,
				Err(problem) => { panic!("Problem loading image from stream."); }
			}
		} else { 
			image::open(&Path::new(&settings.input_filename)).unwrap() 
		};

		// Decode image and do boundary checking to see if any of the args are outside the valid range.
		//let (w,h) = img.dimensions();
		//let color = img.color();
		
		/*
		img = match settings.region {
			Some(rect) => { imageops::crop(&mut img, rect.0, rect.1, rect.2-rect.0, rect.3-rect.1) },
			None => { img },
		};
		*/
		let target_region = imageops::resize(&img, target_width, target_height, FilterType::CatmullRom); // Nearest/Triangle/CatmullRom/Gaussian/Lanczos3
		//for pixel in target_region.pixels() {
		for (x, y, pixel) in target_region.enumerate_pixels() { // TODO: pixel should be yielding x, y, pixel.
			print_color_character('#', pixel.data[0], pixel.data[1], pixel.data[2], settings.use_xterm_colors);
			if x == target_width-1 {
				print!("\n");
			}
		}
	}
}
