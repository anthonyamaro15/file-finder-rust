// get all files from Desktop dir
// have a loop that runs until user selects a file from list or exits
// as user types, filter the file list to show only files that match the user input
// if user selects a file, print the the file path (for now) and exit the program
//

use std::{fs, io};

// TODO :
// Bonus (if user selects a file, paste the file path in terminal or copy to clipboard || open nvim
// with the file path)
// Bonus (Implement ratatui to create a basic UI for the file list)
// Bonus: research how to create a binary/executable from this code and have it run when user types a command in terminal
fn main() -> anyhow::Result<()> {
    println!("Hello, world!");

    let mut entries = fs::read_dir("../../")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    if entries.is_empty() {
        // display message error that something went wrong
    }

    for value in entries.iter() {
        let val = value.clone().into_os_string().to_str().unwrap().to_string();

        if val.contains("web") {
            println!("what is this {}", val);
        }
    }

    //println!("entries here {:?}", entries);

    Ok(())
}
