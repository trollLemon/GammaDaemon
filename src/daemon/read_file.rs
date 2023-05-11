/*  Contains functions related to reading files. We'll mostly read files relating to AC status, or
 *  for other hardware if we need quick information
 *
 * */

use std::fs;
use std::io;

// gets contents of a file
// Returns a Result with a success type of String, otherwise returns an error
pub fn get_contents(path: &str) -> io::Result<String> {
    let data = fs::read_to_string(path)?;
    Ok(data)
}
