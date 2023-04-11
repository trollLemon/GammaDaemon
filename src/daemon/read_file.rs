
use std::fs;
use std::io;





    pub fn get_contents(path: &str) -> io::Result<String> {
        let data = fs::read_to_string(path)?;
        Ok(data)
    }

