#![feature(trim_prefix_suffix)]
use std::{
    cmp::Ordering,
    io::{Cursor, Read},
    process,
};

use bytes::Bytes;
use dll_syringe::{
    Syringe,
    process::{BorrowedProcess, OwnedProcess, ProcessModule},
};
use eframe::egui::{self, OpenUrl, ProgressBar};

pub mod args;

#[derive(Debug)]
struct InjectorApp {
    progress: f32,
    progress_text: String,
}

impl Default for InjectorApp {
    fn default() -> Self {
        Self {
            progress: 0.0,
            progress_text: "IDLE".to_string(),
        }
    }
}

impl eframe::App for InjectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.image(egui::include_image!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/banner.png"
            )));

            ui.label("Note: Please navigate to the cosmetics menu before you inject");
            if self.progress == 0.0 {
                ui.label(&self.progress_text);

                if ui.button("Get free cosmetics").clicked() {
                    let pid = find_minecraft_process();
                    let Some(pid) = pid else {
                        self.progress_text = "Cannot found badlion process".to_string();
                        return;
                    };
                    match inject_to_process(
                        pid,
                        "hook_dll.dll",
                        "org.cubewhy.TweakEntrypoint.init",
                        "",
                        |status_text, progress| {
                            self.progress_text = status_text.to_string();
                            self.progress = progress;
                        },
                    ) {
                        Ok(_) => {
                            self.progress = 1.0;
                        }
                        Err(e) => {
                            eprintln!("Failed to inject {e}");
                            self.progress_text = format!("Failed to inject: {e}");
                            self.progress = 0.0;
                        }
                    };
                };
            } else {
                let progress = ProgressBar::new(self.progress).text(&self.progress_text);
                ui.add(progress);
            }

            if ui.link("lunarclient.top").clicked() {
                OpenUrl::new_tab("https://lunarclient.top");
            }
        });
    }
}

fn find_minecraft_process() -> Option<u32> {
    for p in tasklist::Tasklist::new().unwrap() {
        let pname = p.get_pname();
        if pname != "javaw.exe" && pname != "java.exe" {
            continue;
        }

        let Ok(cmdline) = p.get_cmd_params() else {
            continue;
        };
        if cmdline.contains("--badlionPid") {
            // this is the badlion process
            let pid = p.get_pid();
            return Some(pid);
        }
    }

    None
}

fn main() -> anyhow::Result<()> {
    // resolve args
    // let args = ProgramArgs::parse();

    println!("Cracked by earthsworth LLC in 1 second");
    println!("lunarclient.top");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "Earthsworth Injector",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::<InjectorApp>::default())
        }),
    ) {
        Ok(_) => println!("Ok"),
        Err(_) => {
            println!("Failed to load gui, trying fallback method...");
            // find pid
            let Some(pid) = find_minecraft_process() else {
                eprintln!("Failed to find badlion process");
                process::exit(1);
            };
            match inject_to_process(
                pid,
                "hook_dll.dll",
                "org.cubewhy.TweakEntrypoint.init",
                "",
                |_status_text, _progress| {},
            ) {
                Ok(_) => {
                    println!("Completed injected! Love from earthsworth");
                }
                Err(e) => {
                    eprintln!("Failed to inject {e}");
                }
            };
        }
    }
    ();

    // inject_to_process(args.pid, &args.dll, &args.entrypoint, &args.args)?;

    Ok(())
}

fn inject_to_process(
    pid: u32,
    dll_path: &str,
    entrypoint: &str,
    entry_args: &str,
    mut status_callback: impl FnMut(&str, f32) -> (),
) -> anyhow::Result<()> {
    let Ok(target_process) = OwnedProcess::from_pid(pid) else {
        eprintln!("Cannot found process with pid {}", pid);
        process::exit(1);
    };
    println!("Injecting to process {}", pid);
    status_callback(&format!("Injecting to process {pid}"), 0.2);
    let syringe = Syringe::for_process(target_process);

    // inject to process
    let injected_payload = syringe.inject(dll_path).unwrap();
    status_callback("Attached to progress", 0.4);
    println!("Completed injected");

    // now we injected to the game

    // init dll
    status_callback("Init dll!", 0.5);
    println!("Init dll");
    init_dll(entrypoint, entry_args, &syringe, injected_payload)?;

    // now the dll hooked java
    // we can load classes into the JVM
    status_callback("Loading class to memory!", 0.6);
    println!("Loading classes to memory");
    cache_classes(&syringe, injected_payload)?;

    status_callback("Finally! Loading tweaks!", 0.8);
    println!("Loading tweaks...");
    // call dll to invoke the entrypoint in org.cubewhy.Tweaker class
    install_tweak(&syringe, injected_payload)?;

    status_callback("Completed! Love from earthsworth", 1.0);
    // eject the payload from the target (optional)
    // syringe.eject(injected_payload)?;

    Ok(())
}

fn init_dll(
    entrypoint: &str,
    entry_args: &str,
    syringe: &Syringe,
    injected_payload: ProcessModule<BorrowedProcess<'_>>,
) -> anyhow::Result<()> {
    let remote_init_func =
        unsafe { syringe.get_payload_procedure::<fn() -> ()>(injected_payload, "init") }?.unwrap();
    remote_init_func.call()?;

    let remote_set_entry_point_func = unsafe {
        syringe.get_payload_procedure::<fn(
            class_name: String,
            function_name: String,
            entry_args: String,
        ) -> ()>(injected_payload, "set_entry_point")
    }?
    .unwrap();

    // parse entrypoint
    // org.cubewhy.[Entrypoint] -> org/cubewhy, Entrypoint
    let splitted: Vec<_> = entrypoint.split(".").map(|s| s.to_string()).collect();
    let entry_func_name = splitted.last().unwrap();
    let entry_class_name = entrypoint
        .trim_suffix(format!(".{entry_func_name}").as_str())
        .replace(".", "/")
        .to_string();
    remote_set_entry_point_func.call(
        &entry_class_name,
        &entry_func_name,
        &entry_args.to_string(),
    )?;

    Ok(())
}

fn cache_classes(
    syringe: &Syringe,
    injected_payload: ProcessModule<BorrowedProcess<'_>>,
) -> anyhow::Result<()> {
    let remote_cache_class_func = unsafe {
        syringe
            .get_payload_procedure::<fn(String, Vec<u8>) -> bool>(injected_payload, "cache_class")
    }?
    .unwrap();

    // load the jar
    let jar_bytes: Bytes = Bytes::from_static(include_bytes!(env!("JAR_PATH")));

    // parse the jar
    let mut jar_archive = zip::ZipArchive::new(Cursor::new(jar_bytes))?;

    let mut file_names: Vec<_> = jar_archive.file_names().map(|s| s.to_string()).collect();
    // nested classes should be loaded first
    file_names.sort_by(|item1, item2| {
        if item1.contains("$") && !item2.contains("$") {
            Ordering::Less
        } else if !item1.contains("$") && item2.contains("$") {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    for file_name in file_names {
        let mut entry = jar_archive.by_name(&file_name)?;
        if entry.is_dir() || !file_name.ends_with(".class") {
            continue; // skip dirs
        }
        let class_name = file_name.trim_suffix(".class").to_string();
        // read class bytes
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        // add the class
        // call the function to load class to the jvm
        println!(
            "Add class {class_name} into remote memory ({}bytes)",
            buf.len()
        );
        remote_cache_class_func.call(&class_name, &buf)?;
    }

    // TODO: resources file support

    Ok(())
}

fn install_tweak(
    syringe: &Syringe,
    injected_payload: ProcessModule<BorrowedProcess<'_>>,
) -> anyhow::Result<()> {
    let remote_init_func =
        unsafe { syringe.get_payload_procedure::<fn() -> ()>(injected_payload, "do_tweak") }?
            .unwrap();
    remote_init_func.call()?;

    Ok(())
}
