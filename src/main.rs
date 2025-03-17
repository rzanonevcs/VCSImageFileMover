use notify::{recommended_watcher, Event, RecursiveMode, Result, Watcher, Config};
use std::sync::{mpsc::channel, Mutex, Arc};
use std::time::Duration;
use std::path::Path;
use std::collections::HashSet;
use tokio::fs;

fn main() {
    let watch_files_main = Arc::new(Mutex::new(HashSet::<&Path>::new()));
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <source_dir> <dest_dir>", args[0]);
        std::process::exit(1);
    }

    let source_dir = Path::new(&args[1]);
    let dest_dir = Path::new(&args[2]);

    //Generate a channel which will send events from tx to rx
    let (tx, rx) = channel::<Result<Event>>();
    //Establish a watcher which will watch changes and send event to tx (which will then send them to rx)
    let mut watcher = recommended_watcher(tx).unwrap();
    //Set up the config of the watcher to watch with a poll interval of 0.5s
    let config = Config::default()
        .with_poll_interval(Duration::from_millis(500));
    //Configure the watcher
    watcher.configure(config).unwrap();
    //Start the watcher, with recursive mode to search sub-folders
    watcher.watch(source_dir, RecursiveMode::Recursive).unwrap();

    //Loop waiting for rx to receive a signal
    loop {
        //Waits for rx.recv to return and when it does verifies the match statement.
        match rx.recv() {
            Ok(event) => {
                if let notify::EventKind::Create(_) = event.as_ref().unwrap().kind {
                    for path in event.unwrap().paths {
                        if path.is_file() {
                            let relative_path = path.strip_prefix(source_dir).unwrap();
                            
                            let dest_path = Path::new(dest_dir).join(relative_path);
                            if let Some(parent) = dest_path.parent() {
                                std::fs::create_dir_all(parent).unwrap();
                            }
                            match std::fs::copy(&path, &dest_path) {
                                Ok(_res) => {
                                    std::fs::remove_file(&path).unwrap();
                                }
                                Err(fault) => {
                                    eprintln!("{:?}", fault);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("watch error: {:?}", e),
        }
    }
}