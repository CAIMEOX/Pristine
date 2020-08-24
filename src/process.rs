extern crate gstreamer as gst;
extern crate image;

use glib::subclass;
use glib::subclass::prelude::*;
use gst::subclass::prelude::*;
use gst_video::prelude::*;
use gst_video::subclass::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;

use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use std::path::Path;
use mc_rs::pack::McPack;
use self::gst::gst_sys::GST_PARAM_MUTABLE_READY;
use std::io::Read;

#[derive(Debug, Clone)]
struct Settings {
    width: u32,
    height: u32,
    scale: u32,
    r#loop: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            width: 2,
            height: 20,
            scale: 1,
            r#loop: false,
        }
    }
}

static PROPERTIES: [subclass::Property; 4] = [
    subclass::Property("width", |name| {
        glib::ParamSpec::uint(
            name,
            "Width",
            "The width of the single image",
            1,
            std::u32::MAX,
            1,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("height", |name| {
        glib::ParamSpec::uint(
            name,
            "Height",
            "The height of the single image",
            1,
            std::u32::MAX,
            1,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("scale", |name| {
        glib::ParamSpec::uint(
            name,
            "Scale",
            "The scale of the particle",
            1,
            std::u32::MAX,
            1,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("loop", |name| {
        glib::ParamSpec::boolean(
            name,
            "Loop",
            "Looping animation",
            false,
            glib::ParamFlags::READWRITE,
        )
    }),
];

struct State {
    video_info: gst_video::VideoInfo,
    context: Option<DynamicImage>,
    pack: Option<McPack>,
    path: String,
    width: u32,
    height: u32,
    ptr: (u32, u32),
}
impl State {
    pub fn new(video_info: gst_video::VideoInfo) -> Self {
        Self {
            video_info,
            context: None,
            pack: None,
            path: String::from("tmp"),
            ptr: (0, 0),
            width: 0,
            height: 0,
        }
    }
    pub fn reset(&mut self, settings: Settings) {
        let width = self.video_info.width();
        let height = self.video_info.height();
        self.width = settings.width;
        self.height = settings.height;

        let mut cache = std::fs::File::open(".cache.json").unwrap();
        let mut contents = String::new();
        cache.read_to_string(&mut contents);
        let v: Cache = serde_json::from_str(&contents).unwrap();
        use serde::{Serialize, Deserialize};
        #[derive(Serialize, Deserialize)]
        struct Cache<'a> {
            name: &'a str,
            fcm: &'a str,
            description: &'a str,
        }
        self.new_pack(v.name, v.description);
        if let Some(pack) = &self.pack {
            pack.init_all()
        }
        let img = ImageBuffer::new(width * settings.width, height * settings.height);
        self.context = Some(DynamicImage::ImageRgba8(img));
    }
    fn new_pack(&mut self, name: &str, description: &str) {
        self.pack = Some(McPack::new(name, description));
    }
    fn write_data(&mut self, data: &[u8], rate: u32) {
        if let Some(writer) = self.context.as_mut() {
            let img: RgbaImage = ImageBuffer::from_raw(
                self.video_info.width(),
                self.video_info.height(),
                Vec::from(&data[..]),
            )
            .unwrap();
            image::imageops::replace(
                writer,
                &img,
                self.ptr.0 * self.video_info.width(),
                self.video_info.height() * self.ptr.1,
            );
            println!("w -> {} h -> {}", self.ptr.0, self.ptr.1);
            if self.ptr.1 == self.height - 1 {
                if self.ptr.0 == self.width - 1 {
                    writer
                        .save(&Path::new(&format!(
                            "{}/{}.png",
                            self.path.clone(),
                            rate / (self.height * self.width)
                        )))
                        .expect("Unable to save image");
                    println!("File Saved on PNG::{}", rate / (self.height * self.width));
                    self.ptr.0 = 0;
                } else {
                    self.ptr.0 += 1;
                }

                self.ptr.1 = 0;
            } else {
                self.ptr.1 += 1;
            }
        } else {
            unreachable!()
        }
    }
}

pub struct Encoder {
    state: Mutex<Option<State>>,
    settings: Mutex<Settings>,
}

impl ObjectSubclass for Encoder {
    const NAME: &'static str = "Encoder";
    type ParentType = gst_video::VideoEncoder;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata("Pristine", "Encoder/Video", "Video to frames", "CAIMEO");

        let sink_caps = gst::Caps::new_simple(
            "video/x-raw",
            &[
                (
                    "format",
                    &gst::List::new(&[
                        // &gst_video::VideoFormat::Gray8.to_str(),
                        // &gst_video::VideoFormat::Gray16Be.to_str(),
                        // &gst_video::VideoFormat::Rgb.to_str(),
                        &gst_video::VideoFormat::Rgba.to_str(),
                    ]),
                ),
                ("width", &gst::IntRange::<i32>::new(1, std::i32::MAX)),
                ("height", &gst::IntRange::<i32>::new(1, std::i32::MAX)),
                (
                    "framerate",
                    &gst::FractionRange::new(
                        gst::Fraction::new(1, 1),
                        // frame-delay timing in gif is a multiple of 10ms -> max 100fps
                        gst::Fraction::new(std::i32::MAX, 1),
                    ),
                ),
            ],
        );
        let sink_pad_template = gst::PadTemplate::new(
            "sink",
            gst::PadDirection::Sink,
            gst::PadPresence::Always,
            &sink_caps,
        )
        .unwrap();
        klass.add_pad_template(sink_pad_template);
        let src_caps = gst::Caps::new_simple("image/png", &[]);
        let src_pad_template = gst::PadTemplate::new(
            "src",
            gst::PadDirection::Src,
            gst::PadPresence::Always,
            &src_caps,
        )
        .unwrap();
        klass.add_pad_template(src_pad_template);

        klass.install_properties(&PROPERTIES);
    }

    fn new() -> Self {
        Self {
            state: Mutex::new(None),
            settings: Mutex::new(Default::default()),
        }
    }


}

impl ObjectImpl for Encoder {
    fn set_property(&self, _obj: &glib::Object, id: usize, value: &glib::Value) {
        let prop = &PROPERTIES[id];

        match *prop {
            subclass::Property("width", ..) => {
                let mut settings = self.settings.lock();
                settings.width = value.get_some::<u32>().expect("width must be u32")
            }
            subclass::Property("height", ..) => {
                let mut settings = self.settings.lock();
                settings.height = value.get_some::<u32>().expect("height must be u32")
            }
            subclass::Property("scale", ..) => {
                let mut settings = self.settings.lock();
                settings.scale = value.get_some::<u32>().expect("height must be u32")
            }
            subclass::Property("loop", ..) => {
                let mut settings = self.settings.lock();
                settings.r#loop = value.get_some::<bool>().expect("height must be u32")
            }
            _ => unreachable!(),
        }
    }

    fn get_property(&self, _obj: &glib::Object, id: usize) -> Result<glib::Value, ()> {
        let prop = &PROPERTIES[id];

        match *prop {
            subclass::Property("width", ..) => {
                let settings = self.settings.lock();
                Ok(settings.width.to_value())
            }
            subclass::Property("height", ..) => {
                let settings = self.settings.lock();
                Ok(settings.height.to_value())
            }
            subclass::Property("scale", ..) => {
                let settings = self.settings.lock();
                Ok(settings.scale.to_value())
            }
            subclass::Property("loop", ..) => {
                let settings = self.settings.lock();
                Ok(settings.r#loop.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for Encoder {}

impl VideoEncoderImpl for Encoder {
    fn stop(&self, _element: &gst_video::VideoEncoder) -> Result<(), gst::ErrorMessage> {
        *self.state.lock() = None;
        Ok(())
    }

    fn finish(
        &self,
        _element: &gst_video::VideoEncoder,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_guard = self.state.lock();
        let state = state_guard.as_mut().ok_or(gst::FlowError::NotNegotiated)?;
        if let Some(writer) = state.context.as_mut() {
            let settings = self.settings.lock();
            let ptr = state.ptr;
            let bound = (state.width, state.height);
            let top: RgbaImage =
                image::ImageBuffer::from_fn(settings.width, settings.height, |_, _| {
                    Rgba([225, 225, 225, 225])
                });
            for x in ptr.0..bound.0 {
                for y in ptr.1..bound.1 {
                    println!("{}, {}", x, y);
                    image::imageops::replace(writer, &top, x * settings.width, y * settings.height)
                }
            }

            writer
                .save(&Path::new(&format!("{}/ext.png", state.path.clone())))
                .expect("Unable to save image");
            Ok(gst::FlowSuccess::Ok)
        } else {
            Err(gst::FlowError::NotNegotiated)
        }
    }

    fn set_format(
        &self,
        element: &gst_video::VideoEncoder,
        state: &gst_video::VideoCodecState<'static, gst_video::video_codec_state::Readable>,
    ) -> Result<(), gst::LoggableError> {
        let video_info = state.get_info();
        println!("Setting format {:?}", video_info);
        {
            let mut state = State::new(video_info);
            let settings = self.settings.lock();
            state.reset((*settings).clone());
            *self.state.lock() = Some(state);
        }

        let output_state = element
            .set_output_state(gst::Caps::new_simple("image/png", &[]), Some(state))
            .expect("Failed to set output state");
        element
            .negotiate(output_state)
            .expect("Failed to negotiate!");
        Ok(())
    }

    fn handle_frame(
        &self,
        element: &gst_video::VideoEncoder,
        frame: gst_video::VideoCodecFrame,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_guard = self.state.lock();
        let state = state_guard.as_mut().ok_or(gst::FlowError::NotNegotiated)?;
        if frame.get_system_frame_number() % 40 == 0 {
            println!("Sending frame {}", frame.get_system_frame_number());
        }

        {
            let input_buffer = frame
                .get_input_buffer()
                .expect("frame without input buffer");

            let input_map = input_buffer.map_readable().unwrap();
            let data = input_map.as_slice();
            state.write_data(data, frame.get_system_frame_number());
        }

        drop(state_guard);
        element.finish_frame(Some(frame))
    }
}

pub fn register() -> Result<(), glib::BoolError> {
    gst::Element::register(
        None,
        "Encoder",
        gst::Rank::Primary,
        Encoder::get_type(),
    )
}

