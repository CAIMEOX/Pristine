#[macro_use]
extern crate glib;
extern crate clap;
extern crate serde_json;
use clap::{App, Arg};
use gst::prelude::*;
use gstreamer as gst;
use mc_rs::pack::McPack;
mod process;
use std::time::SystemTime;
use std::fs;
use std::io::Write;

fn main() {
    let matches = App::new("Pristine")
        .version("0.2.0")
        .author("CAIMEO")
        .about("Pristine is a simple-to-use and efficient software used to play video in Minecraft Bedrock.")
        .arg(Arg::with_name("path")
            .short("p")
            .long("path")
            .help("Sets the path of the video.")
            .takes_value(true))
        .arg(Arg::with_name("name")
            .short("n")
            .long("name")
            .help("Sets the pack\'s name.")
            .takes_value(true)
        )
        .arg(Arg::with_name("description")
            .short("d")
            .long("description")
            .help("Sets the description")
            .takes_value(true))
        .arg(Arg::with_name("fps")
            .long("fps")
            .help("Sets frames per second")
            .takes_value(true))
        .arg(Arg::with_name("spi")
            .long("spi")
            .help("Sets seconds per image")
            .takes_value(true))
        .arg(Arg::with_name("loop")
            .short("l")
            .help("Sets looping animation"))
        .arg(Arg::with_name("scale")
            .long("scale")
            .short("s")
            .help("The particle scale")
            .takes_value(true))
        .arg(Arg::with_name("facing-camera-mode")
            .long("facing-camera-mode")
            .short("fcm")
            .help("Sets facing camera mode")
            .takes_value(true))
        .get_matches();

    let path = matches.value_of("path").expect("Missing flag <path>!");
    let name = matches.value_of("name").or(Some("pristine")).unwrap();
    let fps = matches.value_of("fps").unwrap_or("20").parse::<u32>().unwrap_or(20);
    let spi = matches.value_of("spi").unwrap_or("1").parse::<u32>().unwrap_or(1);
    let scale = matches.value_of("scale").unwrap_or("1").parse::<u32>().unwrap_or(1);
    let fcm = matches.value_of("facing-camera-mode").unwrap_or("lookat_xyz");
    let looping = matches.occurrences_of("loop") > 0;
    let description = matches
        .value_of("description")
        .or(Some("Pristine Video Pack.Powered by CAIMEO. LICENSE MIT."))
        .unwrap();

    let mut r = fs::File::create(".cache.json").unwrap();
    r.write_all(&serde_json::to_string(&serde_json::json!(
        {
            "name":name,
            "description":description,
            "fcm":fcm,
        }
    )).unwrap().as_ref());

    gst::init().expect("Unable to init gstreamer.");
    process::register().expect("Unable to register plugin.");
    let t1 = SystemTime::now();
    let encode_pipeline: &str = &format!(
        "filesrc location={} ! decodebin ! videoconvert ! Encoder width={} height={} scale={} loop={} ! filesink location=.cache",
        path,fps, spi, scale, looping
    );

    println!("{}", encode_pipeline);
    let pipeline = gst::parse_launch(encode_pipeline).unwrap();
    // let pipeline = pipeline.dynamic_cast::<gst::Pipeline>().unwrap();
    // let enc = pipeline.get_by_name("enc").unwrap();
    //
    // enc.set_property("height", &fps);
    // enc.set_property("width", &spi);
    // enc.set_property("name", &name);
    // enc.set_property("description", &description);
    // enc.set_property("loop", &looping);
    pipeline
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    let bus = pipeline.get_bus().unwrap();
    for msg in bus.iter_timed(gst::CLOCK_TIME_NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Error(err) => {
                eprintln!(
                    "Error received from element {:?}: {}",
                    err.get_src().map(|s| s.get_path_string()),
                    err.get_error()
                );
                eprintln!("Debugging information: {:?}", err.get_debug());
                break;
            }
            MessageView::Eos(..) => break,
            _ => (),
        }
    }

    pipeline
        .set_state(gst::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");
    println!(
        "Time used: {}",
        SystemTime::now().duration_since(t1).unwrap().as_millis()
    )
}
