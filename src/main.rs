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
    let dest_dir_str = args[2].clone();
    let dest_dir = Path::new(&dest_dir_str);

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
    let dest_dir_clone = dest_dir.clone();

    let mut paths_handles = std::collections::HashMap::<PathBuf, JoinHandle<()>>::new();
    let rt_in_hashmap_adder = rt_in_hashmap.clone();
    rt_in_hashmap.clone().spawn( async move {
        loop {
            let sent_path= path_receiver.recv().await.unwrap();
            paths_handles.entry(sent_path).or_insert_with_key(|path| {
                let owned_path_clone = path.clone();
                let dest_dir_inner= dest_dir_clone.clone();
                rt_in_hashmap_adder.clone().spawn( async move {
                    let path_clone = owned_path_clone.as_path();
                    let dest_path = create_dest_path_from_relative(path_clone, dest_dir_inner);
                    println!("Path: {:?}", path_clone);
                    if let Some(parent) = dest_path.parent() {
                        tokio::fs::create_dir_all(parent).await.unwrap();
                    }
                    match tokio::fs::copy(path_clone, dest_path).await {
                        Ok(_res) => {
                            tokio::fs::remove_file(path_clone).await.unwrap();
                        }
                        Err(fault) => {
                            eprintln!("{:?}", fault);
                        }
                    }
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
                            }
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {:?}", e),
            }
        }
    });
}

fn create_dest_path_from_relative<'a>(rel_path : &'a Path, dest_dir_path : &Path) -> PathBuf {
    return Path::new(dest_dir_path).join(rel_path);
}