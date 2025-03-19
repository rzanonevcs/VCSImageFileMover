use notify::{recommended_watcher, Event, RecursiveMode, Result, Watcher, Config};
use std::sync::{Mutex, Arc};
use tokio::sync::mpsc::unbounded_channel;
use std::time::Duration;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use tokio::{fs, runtime::{Runtime, Builder}, task::JoinHandle};

struct EventHandledTokioSender(tokio::sync::mpsc::UnboundedSender<Result<Event>>);

impl notify::EventHandler for EventHandledTokioSender {
    fn handle_event(&mut self, event: Result<Event>) {
        let _ = self.0.send(event);
    }
}

impl std::ops::Deref for EventHandledTokioSender {
    type Target = tokio::sync::mpsc::UnboundedSender<Result<Event>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <source_dir> <dest_dir>", args[0]);
        std::process::exit(1);
    }

    let source_dir = Path::new(&args[1]);
    let dest_dir = Path::new(&args[2]);

    //Generate a channel which will send events from tx to rx
    let (watcher_tx, mut watcher_rx) = unbounded_channel::<Result<Event>>();
    //Establish a watcher which will watch changes and send event to tx (which will then send them to rx)
    let mut watcher = recommended_watcher(EventHandledTokioSender(watcher_tx)).unwrap();
    //Set up the config of the watcher to watch with a poll interval of 0.5s
    let config = Config::default()
        .with_poll_interval(Duration::from_millis(500));
    //Configure the watcher
    watcher.configure(config).unwrap();
    //Start the watcher, with recursive mode to search sub-folders
    watcher.watch(source_dir, RecursiveMode::Recursive).unwrap();

    //Create the tokio runtime which will handle watching for events
    let mut rt = Arc::new(Builder::new_multi_thread().worker_threads(2).build().unwrap());
    
    //Spawn the task which copies the files
    let (path_sender, mut path_receiver) = unbounded_channel::<PathBuf>();
    let rt_in_hashmap = rt.clone();
    let mut paths_handles = std::collections::HashMap::<PathBuf, JoinHandle<()>>::new();
    let rt_in_hashmap_adder = rt_in_hashmap.clone();
    rt_in_hashmap.clone().spawn( async move {
        loop {
            let sent_path= path_receiver.recv().await.unwrap();
            paths_handles.entry(sent_path.to_path_buf()).or_insert_with_key(|path| {
                let path_clone = path.clone();
                rt_in_hashmap_adder.clone().spawn_blocking( move || {
                    println!("Path: {:?}", path_clone)
                })
            });
        }
    });

    // Create the path watcher
    rt.block_on(async {
        //Loop waiting for rx to receive a signal
        loop {
            //Waits for rx.recv to return and when it does verifies the match statement.
            match watcher_rx.recv().await.unwrap() {
                Ok(event) => {
                    if let notify::EventKind::Create(_) = event.kind {
                        for path in event.paths {
                            if path.is_file() {
                                let relative_path = path.strip_prefix(source_dir).unwrap();
                                path_sender.send(relative_path.to_owned()).unwrap();

                                /*
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
                                */
                            }
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {:?}", e),
            }
        }
    });
}

fn create_dest_path_from_relative<'a>(rel_path : &'a Path, dest_dir : &Path) -> PathBuf {
    return Path::new(dest_dir).join(rel_path);
}