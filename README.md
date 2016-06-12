# catpicture
#### It cats pictures!
A command line tool for dumping a preview of an image on a remote machine to the command line.
Useful for converting a picture to a thousand words, assuming ```word ∈ [ -~]*.```
Also handy for making fun ascii art from pictures.

## Usage

cat imagename | catpicture [args]

OR

catpicture [args] <image name>

OR 

curl -sv imageurl | catpicture [args]

## Command Line Arguments

* -w Specify output width (default 80).
* -h Output height.
* -r <x y w h> - Select a sub-rectancle with the given dimensions.
* -c Try to display full color.
* -g Force greyscale. (Try using with -c to get full grey range if your terminal supports it.)
* -d <none|block|char c|line> Change drawing mode. None will fill background. Block will fill with '#'. Char will fill with the given character.  Line will try to draw lines.

## Release Plan

* v0.1.0 : First release -- Supports just '#' for output style.  Allows -c for full-color mode, -r, -w, -h to change sizes.
* v0.2.0 : Automatically select correct aspect ratio when only -w or -h supplied.  Support force-grey.
* (Current) v0.3.0 : Add new line algorithms with -d.  Can fill BG instead of '#', supports BG, '#', and gradient.
* v0.4.0 : Use nearest neighbor to select the best looking ascii stand-in.
* v0.5.0 : Hardening and improvements to robustness.  Bounds checking.  Ready for beta release.
* v0.6.0 : Allow threshold to be set for _not_ drawing, so if people want black text to show as empty space (for writing to text file), that can be done.
* v1.0.0 : Ready for release.

