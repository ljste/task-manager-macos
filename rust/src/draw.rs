use crate::{model::*, state::*};
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2::{msg_send, rc::Retained};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
};
extern "C" {
    fn tm_icon_thumbnail(path: &NSString, pixels: usize) -> *mut NSImage;
}

fn icon_thumbnail(path: &NSString, pixels: usize) -> Option<Retained<NSImage>> {
    unsafe { Retained::from_raw(tm_icon_thumbnail(path, pixels)) }
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(0.), h.max(0.)))
}
pub fn nsrect(r: [f64; 4]) -> NSRect {
    rect(r[0], r[1], r[2], r[3])
}
pub fn rgb(r: f64, g: f64, b: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.)
}
pub fn gray(v: f64) -> Retained<NSColor> {
    rgb(v, v, v)
}
pub struct Painter {
    chrome: Option<Retained<NSGradient>>,
    heat_colors: [Option<Retained<NSColor>>; 256],
    image_order: VecDeque<String>,
    pub fonts: HashMap<(u16, bool), Retained<NSFont>>,
    pub images: HashMap<String, Retained<NSImage>>,
    pub pending_images: std::collections::VecDeque<String>,
    pub attributes:
        HashMap<(u16, bool, bool, u64, u64, u64), Retained<NSDictionary<NSString, AnyObject>>>,
}
impl Painter {
    pub fn new(_mtm: MainThreadMarker) -> Self {
        Self {
            chrome: NSGradient::initWithStartingColor_endingColor(
                NSGradient::alloc(),
                &rgb(0.937, 0.953, 0.973),
                &rgb(0.902, 0.953, 0.949),
            ),
            heat_colors: std::array::from_fn(|_| None),
            image_order: VecDeque::new(),
            fonts: HashMap::new(),
            images: HashMap::new(),
            pending_images: std::collections::VecDeque::new(),
            attributes: HashMap::new(),
        }
    }
    pub fn font(&mut self, size: f64, bold: bool) -> Retained<NSFont> {
        self.fonts
            .entry((size as u16, bold))
            .or_insert_with(|| {
                NSFont::fontWithName_size(
                    &NSString::from_str(if bold { "SegoeUI-Semibold" } else { "SegoeUI" }),
                    size,
                )
                .or_else(|| {
                    NSFont::fontWithName_size(
                        &NSString::from_str(if bold {
                            "Selawik-Semibold"
                        } else {
                            "Selawik-Regular"
                        }),
                        size,
                    )
                })
                .unwrap_or_else(|| {
                    NSFont::systemFontOfSize_weight(size, if bold { 0.3 } else { 0. })
                })
            })
            .clone()
    }
    pub fn text(
        &mut self,
        s: &str,
        r: [f64; 4],
        size: f64,
        bold: bool,
        color: &NSColor,
        right: bool,
    ) {
        if s.is_empty() || r[2] < 1. {
            return;
        }
        let key = (
            size as u16,
            bold,
            right,
            color.redComponent().to_bits(),
            color.greenComponent().to_bits(),
            color.blueComponent().to_bits(),
        );
        if !self.attributes.contains_key(&key) {
            let font = self.font(size, bold);
            let p = NSMutableParagraphStyle::new();
            p.setAlignment(if right {
                NSTextAlignment::Right
            } else {
                NSTextAlignment::Left
            });
            p.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            let attrs = unsafe {
                NSDictionary::<NSString, AnyObject>::from_slices(
                    &[
                        NSFontAttributeName,
                        NSForegroundColorAttributeName,
                        NSParagraphStyleAttributeName,
                    ],
                    &[&*font as &AnyObject, color as &AnyObject, &*p as &AnyObject],
                )
            };
            self.attributes.insert(key, attrs);
        }
        let attrs = self.attributes.get(&key).unwrap();
        unsafe {
            let _: () =
                msg_send![&*NSString::from_str(s),drawInRect:nsrect(r),withAttributes:&**attrs];
        }
    }
    pub fn fill(&self, r: [f64; 4], c: &NSColor, radius: f64) {
        c.setFill();
        if radius == 0. {
            NSBezierPath::fillRect(nsrect(r));
            return;
        }
        let p = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(nsrect(r), radius, radius);
        p.fill();
    }
    pub fn line(&self, pts: &[(f64, f64)], c: &NSColor, width: f64) {
        if pts.is_empty() {
            return;
        }
        let p = NSBezierPath::bezierPath();
        p.setLineWidth(width);
        p.moveToPoint(NSPoint::new(pts[0].0, pts[0].1));
        for &(x, y) in &pts[1..] {
            p.lineToPoint(NSPoint::new(x, y));
        }
        c.setStroke();
        p.stroke();
    }
    pub fn outline(&self, r: [f64; 4], c: &NSColor, width: f64, radius: f64) {
        let p = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(nsrect(r), radius, radius);
        p.setLineWidth(width);
        c.setStroke();
        p.stroke();
    }
    pub fn icon(&self, name: &str, x: f64, y: f64, size: f64, c: &NSColor) {
        let sc = size / 24.;
        let line = |points: &[(f64, f64)]| {
            self.line(
                &points
                    .iter()
                    .map(|&(a, b)| (x + a * sc, y + b * sc))
                    .collect::<Vec<_>>(),
                c,
                1.35 * sc,
            )
        };
        let boxy = |a: f64, b: f64, w: f64, h: f64| {
            self.outline(
                [x + a * sc, y + b * sc, w * sc, h * sc],
                c,
                1.35 * sc,
                1.5 * sc,
            )
        };
        let circle = |a: f64, b: f64, r: f64| {
            let p = NSBezierPath::bezierPathWithOvalInRect(rect(
                x + (a - r) * sc,
                y + (b - r) * sc,
                r * 2. * sc,
                r * 2. * sc,
            ));
            p.setLineWidth(1.35 * sc);
            c.setStroke();
            p.stroke();
        };
        match name {
            "0" | "processes" => {
                boxy(3., 3., 18., 9.);
                boxy(3., 3., 8., 18.);
            }
            "1" => {
                boxy(2.5, 2.5, 19., 19.);
                line(&[
                    (5., 13.),
                    (8., 13.),
                    (10., 7.),
                    (13., 17.),
                    (15., 11.),
                    (19., 11.),
                ]);
            }
            "2" => {
                let p = NSBezierPath::bezierPath();
                p.setLineWidth(1.35 * sc);
                p.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle_clockwise(
                    NSPoint::new(x + 12. * sc, y + 12. * sc),
                    10. * sc,
                    220.,
                    185.,
                    false,
                );
                c.setStroke();
                p.stroke();
                line(&[(2., 3.), (2., 8.), (7., 8.)]);
                line(&[(12., 6.), (12., 12.), (17., 12.)]);
            }
            "3" => {
                let p = NSBezierPath::bezierPath();
                p.setLineWidth(1.35 * sc);
                p.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle_clockwise(
                    NSPoint::new(x + 12. * sc, y + 15. * sc),
                    10. * sc,
                    180.,
                    360.,
                    false,
                );
                c.setStroke();
                p.stroke();
                for pts in [
                    vec![(3., 14.), (6., 14.)],
                    vec![(5., 7.), (7., 9.)],
                    vec![(12., 5.), (12., 8.)],
                    vec![(19., 7.), (17., 9.)],
                    vec![(18., 15.), (22., 15.)],
                    vec![(10., 17.), (16., 6.), (14., 19.), (10., 17.)],
                ] {
                    line(&pts)
                }
            }
            "4" => {
                circle(8., 7., 4.);
                circle(18., 8., 3.);
                boxy(2., 14., 13., 7.);
                line(&[(17., 14.), (22., 14.), (22., 18.), (20., 21.), (17., 21.)]);
            }
            "5" => {
                for yy in [5., 12., 19.] {
                    circle(2.5, yy, 0.6);
                    line(&[(7., yy), (22., yy)]);
                }
            }
            "6" => {
                let p = NSBezierPath::bezierPath();
                p.setLineWidth(1.35 * sc);
                let pt = |a: f64, b: f64| NSPoint::new(x + a * sc, y + b * sc);
                p.moveToPoint(pt(8.5, 5.));
                p.lineToPoint(pt(8.5, 3.5));
                p.curveToPoint_controlPoint1_controlPoint2(
                    pt(15.5, 3.5),
                    pt(8.5, 0.2),
                    pt(15.5, 0.2),
                );
                for (a, b) in [(15.5, 5.), (19., 5.), (19., 9.)] {
                    p.lineToPoint(pt(a, b))
                }
                p.curveToPoint_controlPoint1_controlPoint2(pt(19., 15.), pt(13., 8.), pt(13., 16.));
                for (a, b) in [(19., 19.), (15.5, 19.), (15.5, 20.5)] {
                    p.lineToPoint(pt(a, b))
                }
                p.curveToPoint_controlPoint1_controlPoint2(
                    pt(8.5, 20.5),
                    pt(15.5, 23.8),
                    pt(8.5, 23.8),
                );
                for (a, b) in [(8.5, 19.), (5., 19.), (5., 15.), (3.5, 15.)] {
                    p.lineToPoint(pt(a, b))
                }
                p.curveToPoint_controlPoint1_controlPoint2(pt(3.5, 9.), pt(0.2, 15.), pt(0.2, 9.));
                for (a, b) in [(5., 9.), (5., 5.)] {
                    p.lineToPoint(pt(a, b))
                }
                p.closePath();
                c.setStroke();
                p.stroke();
            }
            "7" => {
                let mut pts = vec![];
                for i in 0..33 {
                    let a = i as f64 * std::f64::consts::PI / 16.;
                    let r = if i % 4 == 0 || i % 4 == 3 { 8. } else { 11. };
                    pts.push((12. + a.cos() * r, 12. + a.sin() * r));
                }
                line(&pts);
                circle(12., 12., 4.);
            }
            "hamburger" => {
                for yy in [5., 12., 19.] {
                    line(&[(2., yy), (22., yy)])
                }
            }
            "search" => {
                circle(10., 10., 7.);
                line(&[(15., 15.), (22., 22.)]);
            }
            "close" => {
                line(&[(6., 6.), (18., 18.)]);
                line(&[(6., 18.), (18., 6.)]);
            }
            "max" => boxy(5., 5., 14., 14.),
            "restore" => {
                boxy(5., 8., 11., 11.);
                line(&[(8., 8.), (8., 5.), (19., 5.), (19., 16.), (16., 16.)]);
            }
            "min" => line(&[(5., 12.), (19., 12.)]),
            "right" => line(&[(9., 5.), (16., 12.), (9., 19.)]),
            "down" => line(&[(5., 9.), (12., 16.), (19., 9.)]),
            "check" => line(&[(3., 12.), (9., 18.), (21., 5.)]),
            "end" => {
                circle(12., 12., 10.);
                line(&[(5., 19.), (19., 5.)]);
            }
            "run" => {
                boxy(2., 2., 17., 17.);
                line(&[(9., 2.), (9., 10.), (2., 10.)]);
                circle(17., 17., 6.);
                line(&[(17., 13.), (17., 21.)]);
                line(&[(13., 17.), (21., 17.)]);
            }
            "leaf" => {
                let p = NSBezierPath::bezierPath();
                p.setLineWidth(1.35 * sc);
                let pt = |a: f64, b: f64| NSPoint::new(x + a * sc, y + b * sc);
                p.moveToPoint(pt(12., 20.));
                p.curveToPoint_controlPoint1_controlPoint2(pt(22., 4.), pt(24., 22.), pt(23., 10.));
                p.curveToPoint_controlPoint1_controlPoint2(pt(12., 20.), pt(8., 1.), pt(8., 12.));
                c.setStroke();
                p.stroke();
                line(&[(12., 20.), (18., 10.)]);
                line(&[
                    (9., 16.),
                    (5., 14.),
                    (2., 9.),
                    (2., 2.),
                    (9., 2.),
                    (13., 5.),
                ]);
            }
            "service.item" => {
                for (cx, cy, r) in [(8., 8., 7.5), (17., 17., 5.5)] {
                    let mut pts = vec![];
                    for i in 0..41 {
                        let a = i as f64 * std::f64::consts::PI / 20.;
                        let rad = r * if i % 4 == 0 || i % 4 == 3 { 0.78 } else { 1. };
                        pts.push((cx + a.cos() * rad, cy + a.sin() * rad));
                    }
                    let pts: Vec<_> = pts.iter().map(|&(a, b)| (x + a * sc, y + b * sc)).collect();
                    self.line(&pts, &rgb(0.38, 0.53, 0.62), 0.9 * sc);
                    circle(cx, cy, r * 0.35);
                }
            }
            _ => {
                boxy(3., 3., 18., 18.);
                line(&[(3., 8.), (21., 8.)]);
            }
        }
    }
    fn thumbnail(image: &NSImage) -> Option<Retained<NSImage>> {
        let bitmap = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(), std::ptr::null_mut(), 36, 36, 8, 4, true, false, NSDeviceRGBColorSpace, 0, 0)?
        };
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        image.drawInRect(rect(0., 0., 36., 36.));
        NSGraphicsContext::restoreGraphicsState_class();
        let size = NSSize::new(18., 18.);
        bitmap.setSize(size);
        let icon = NSImage::initWithSize(NSImage::alloc(), size);
        icon.addRepresentation(&bitmap);
        Some(icon)
    }

    fn bundle_artwork(bundle_path: &Path) -> Option<Retained<NSImage>> {
        let contents = bundle_path.join("Contents");
        let info = plist::Value::from_file(contents.join("Info.plist")).ok()?;
        let name = info.as_dictionary()?.get("CFBundleIconFile")?.as_string()?;
        let name = Path::new(name).file_name()?;
        let file = contents.join("Resources").join(name).with_extension("icns");
        if !file.is_file() {
            return None;
        }
        icon_thumbnail(&NSString::from_str(file.to_str()?), 36)
    }

    pub fn load_image(&mut self, path: &str) {
        if !self.images.contains_key(path) {
            let img = if path == "appicon" {
                let p = NSBundle::mainBundle()
                    .resourcePath()
                    .map(|p| format!("{p}/AppIcon.icns"))
                    .unwrap_or_default();
                icon_thumbnail(&NSString::from_str(&p), 32)
            } else {
                let file = NSString::from_str(path);
                let artwork = Self::bundle_artwork(Path::new(path)).or_else(|| {
                    let bundle = NSBundle::bundleWithPath(&file)?;
                    let name = bundle
                        .objectForInfoDictionaryKey(ns_string!("CFBundleIconFile"))?
                        .downcast::<NSString>()
                        .ok()?;
                    let stem = name.stringByDeletingPathExtension();
                    let icon_path =
                        bundle.pathForResource_ofType(Some(&stem), Some(ns_string!("icns")))?;
                    icon_thumbnail(&icon_path, 36)
                });
                artwork
                    .or_else(|| Self::thumbnail(&NSWorkspace::sharedWorkspace().iconForFile(&file)))
            };
            if let Some(img) = img {
                if path != "appicon" {
                    if self.image_order.len() >= 128 {
                        if let Some(oldest) = self.image_order.pop_front() {
                            self.images.remove(&oldest);
                        }
                    }
                    self.image_order.push_back(path.into());
                }
                self.images.insert(path.into(), img);
            }
        }
    }
    pub fn warm_images(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..2 {
            if let Some(path) = self.pending_images.pop_front() {
                self.load_image(&path);
                changed = true;
            } else {
                break;
            }
        }
        changed
    }
    pub fn image(&mut self, path: &str, r: [f64; 4]) {
        if path.is_empty() {
            return;
        }
        if !self.images.contains_key(path) && !self.pending_images.iter().any(|p| p == path) {
            self.pending_images.push_back(path.to_owned());
        }
        if let Some(img) = self.images.get(path) {
            unsafe {
                let _: () = msg_send![&**img,drawInRect:nsrect(r),fromRect:rect(0.,0.,0.,0.),operation:2usize,fraction:1.0f64,respectFlipped:true,hints:std::ptr::null::<AnyObject>()];
            }
        }
    }
}
fn hit(s: &mut State, r: [f64; 4], a: Action, label: &str, en: bool) {
    s.hits.push(Hit {
        rect: r,
        action: a,
        label: label.into(),
        enabled: en,
    });
}
fn control(
    p: &mut Painter,
    s: &mut State,
    r: [f64; 4],
    title: &str,
    icon: &str,
    a: Action,
    en: bool,
) {
    let dark = s.prefs.dark;
    if s.hover == a && en {
        p.fill(r, &gray(if dark { 0.22 } else { 0.92 }), 4.);
    }
    let c = gray(if en {
        if dark {
            0.94
        } else {
            0.12
        }
    } else if dark {
        0.42
    } else {
        0.66
    });
    if !icon.is_empty() {
        p.icon(icon, r[0] + 10., r[1] + 8., 18., &c)
    }
    p.text(
        title,
        [
            r[0] + if icon.is_empty() { 10. } else { 36. },
            r[1] + 8.,
            r[2] - if icon.is_empty() { 20. } else { 42. },
            22.,
        ],
        12.,
        false,
        &c,
        false,
    );
    hit(s, r, a, title, en);
}
pub fn render(p: &mut Painter, s: &mut State) {
    s.hits.clear();
    let (w, h) = (s.width, s.height);
    let side = s.sidebar();
    let dark = s.prefs.dark;
    let fg = gray(if dark { 0.94 } else { 0.12 });
    let muted = gray(if dark { 0.65 } else { 0.45 });
    let bg = gray(if dark { 0.09 } else { 1. });
    let rule = gray(if dark { 0.25 } else { 0.88 });
    let accent = rgb(0.0, 0.40, 0.64);
    if let Some(gradient) = p.chrome.as_ref().filter(|_| !dark) {
        gradient.drawInRect_angle(rect(0., 0., w, h), 90.);
    } else {
        p.fill([0., 0., w, h], &gray(if dark { 0.125 } else { 0.95 }), 0.);
    }
    if !s.graph_only {
        p.image("appicon", [52., 17., 16., 16.]);
        p.text(
            "Task Manager",
            [78., 16., 200., 24.],
            12.,
            false,
            &fg,
            false,
        );
        p.icon("hamburger", 16., 15., 22., &fg);
        hit(
            s,
            [0., 0., 46., 48.],
            Action::Collapse,
            "Open navigation",
            true,
        );
        for (i, ic) in ["min", "max", "close"].iter().enumerate() {
            let a = [Action::Minimize, Action::Maximize, Action::Close][i].clone();
            let r = [w - 138. + 46. * i as f64, 0., 46., 36.];
            if s.hover == a {
                p.fill(
                    r,
                    &*if i == 2 {
                        rgb(0.77, 0.17, 0.16)
                    } else {
                        gray(if dark { 0.25 } else { 0.89 })
                    },
                    0.,
                )
            }
            p.icon(
                ic,
                r[0] + 15.,
                10.,
                16.,
                &*if s.hover == a && i == 2 {
                    gray(1.)
                } else {
                    fg.clone()
                },
            );
            hit(
                s,
                r,
                a,
                ["Minimize", "Maximize or restore", "Close"][i],
                true,
            );
        }
        for (i, name) in PAGES.iter().enumerate() {
            let y = if i == 7 {
                h - 48.
            } else {
                49. + i as f64 * 42.
            };
            let a = Action::Page(i);
            if s.page == i || s.hover == a {
                p.fill(
                    [4., y + 3., side - 8., 36.],
                    &gray(if dark {
                        0.21
                    } else if s.page == i {
                        0.91
                    } else {
                        0.93
                    }),
                    4.,
                );
            }
            if s.page == i {
                p.fill([4., y + 13., 3., 16.], &accent, 1.5);
            }
            p.icon(&i.to_string(), 18., y + 11., 18., &fg);
            if !s.collapsed {
                p.text(
                    name,
                    [44., y + 11., side - 52., 22.],
                    13.,
                    false,
                    &fg,
                    false,
                )
            }
            hit(s, [4., y + 3., side - 8., 36.], a, name, true);
        }
        p.fill([side, 48., w - side, h - 48.], &bg, 8.);
        p.fill([side, 56., w - side, h - 56.], &bg, 0.);
        if s.page != 7 {
            p.fill(
                [side, 48., w - side, 48.],
                &*if dark {
                    gray(0.145)
                } else {
                    rgb(0.971, 0.98, 0.985)
                },
                6.,
            );
            p.line(&[(side, 95.5), (w, 95.5)], &rule, 1.);
            p.text(
                PAGES[s.page],
                [side + 16., 64., 170., 24.],
                14.,
                true,
                &fg,
                false,
            );
            let mut commands = vec![("Run new task", "run", Action::Run, true, 136.)];
            match s.page {
                0 => {
                    commands.push(("End task", "end", Action::End, s.can_end(), 105.));
                    commands.push(("Efficiency mode", "leaf", Action::Efficiency, !s.busy, 157.));
                }
                5 => {
                    commands.push(("End task", "end", Action::End, s.can_end(), 105.));
                    commands.push((
                        "Properties",
                        "",
                        Action::Properties,
                        s.current().is_some(),
                        110.,
                    ));
                }
                2 => {
                    commands.push(("Open app", "", Action::Switch, s.current().is_some(), 100.));
                }
                3 => {
                    let enable = s
                        .current()
                        .and_then(|r| r.service)
                        .map(|i| s.services[i].disabled)
                        .unwrap_or(false);
                    let mutable = s
                        .current()
                        .and_then(|r| r.service)
                        .is_some_and(|i| s.services[i].mutable);
                    commands.push((
                        "Enable",
                        "check",
                        Action::Service("enable".into()),
                        mutable && enable,
                        90.,
                    ));
                    commands.push((
                        "Disable",
                        "end",
                        Action::Service("disable".into()),
                        mutable && !enable,
                        100.,
                    ));
                    commands.push((
                        "Properties",
                        "",
                        Action::Properties,
                        s.current().is_some(),
                        100.,
                    ));
                }
                4 => {
                    commands.push(("Disconnect", "4", Action::None, false, 120.));
                    commands.push((
                        "Manage user accounts",
                        "4",
                        Action::Option("manage-users".into()),
                        true,
                        190.,
                    ));
                }
                6 => {
                    let mutable = s
                        .current()
                        .and_then(|r| r.service)
                        .is_some_and(|i| s.services[i].mutable);
                    commands.push(("Start", "", Action::Service("start".into()), mutable, 65.));
                    commands.push(("Stop", "", Action::Service("stop".into()), mutable, 65.));
                    commands.push((
                        "Restart",
                        "",
                        Action::Service("restart".into()),
                        mutable,
                        75.,
                    ));
                    commands.push((
                        "Open Services",
                        "",
                        Action::Option("open-services".into()),
                        true,
                        115.,
                    ));
                }
                _ => {}
            }
            commands.push(("View  ⌄", "", Action::View, true, 70.));
            if commands.iter().map(|v| v.4).sum::<f64>() > w - side - 155. {
                commands = vec![
                    ("Run new task", "run", Action::Run, true, 136.),
                    ("⋯", "", Action::View, true, 44.),
                ];
            }
            let total: f64 = commands.iter().map(|v| v.4).sum();
            let mut x = (w - total - 16.).max(side + 140.);
            for (title, ic, a, en, width) in commands {
                if a == Action::End {
                    p.line(&[(x - 3.5, 59.), (x - 3.5, 85.)], &rule, 0.8);
                }
                if x + width <= w {
                    control(p, s, [x, 55., width, 34.], title, ic, a, en)
                }
                x += width;
            }
        }
    }
    if s.page == 1 {
        performance(p, s)
    } else if s.page == 7 {
        settings(p, s)
    } else {
        table(p, s, None)
    }
    if s.prefs.speed == 0 && !s.graph_only {
        p.text(
            "Paused",
            [side + 120., 67., 70., 19.],
            10.,
            false,
            &muted,
            false,
        );
    }
    if !s.graph_only && [0, 3, 4, 5, 6].contains(&s.page) && s.dialog.is_none() {
        let width = (s.width - 630.).clamp(240., 500.);
        let x = (s.width - width) / 2.;
        p.fill([x, 9., width, 31.], &gray(if dark { 0.18 } else { 1. }), 4.);
        p.outline([x, 9., width, 31.], &rule, 0.8, 4.);
        p.line(&[(x + 3., 39.5), (x + width - 3., 39.5)], &gray(0.62), 0.8);
        if s.query.is_empty() {
            p.text(
                "Type a name, publisher, or PID to search",
                [x + 12., 16., width - 52., 22.],
                12.,
                false,
                &gray(0.48),
                false,
            );
        }
        p.icon("search", x + width - 29., 16., 18., &fg);
        if !s.query.is_empty() {
            p.icon("close", x + width - 51., 18., 14., &fg);
            hit(
                s,
                [x + width - 56., 10., 24., 29.],
                Action::Option("search-clear".into()),
                "Clear search",
                true,
            );
        }
    }
    menus(p, s);
    dialog(p, s);
}
pub fn metric_heading(s: &State, key: &str) -> String {
    match key {
        "cpu" => format!("{:.0}%", s.system.cpu),
        "memory" => format!("{:.0}%", s.system.memory()),
        "disk" => format!(
            "{}/s",
            bytes(s.system.read + s.system.write, s.prefs.binary)
        ),
        "network" => format!(
            "{}/s",
            bytes(s.system.sent + s.system.received, s.prefs.binary)
        ),
        _ => String::new(),
    }
}

#[derive(PartialEq, Eq)]
pub struct MetricCell {
    text: String,
    heat: Option<u8>,
}

impl MetricCell {
    pub fn new(s: &State, row: &Row, key: &str) -> Self {
        let scale = match key {
            "cpu" => Some(100.),
            "memory" => Some(s.system.raw.ram as f64),
            "disk" => Some(s.system.read + s.system.write),
            "network" => Some(1.),
            _ => None,
        };
        let heat = scale.map(|scale| {
            let value = s.number(row, key).unwrap_or(0.);
            let base = if s.selected.as_ref() == Some(&row.id) || s.selections.contains(&row.id) {
                0.37
            } else if matches!(&s.hover, Action::Select(id) if id == &row.id) {
                0.30
            } else {
                0.19
            };
            let alpha = base + 0.46 * (value / scale.max(1.)).clamp(0., 1.).sqrt();
            (alpha * if s.prefs.dark { 0.65 } else { 1. } * 255.).round() as u8
        });
        Self {
            text: s.row_value(row, key).into_owned(),
            heat,
        }
    }
}

fn row_background(dark: bool, selected: bool, hover: bool) -> f64 {
    match (dark, selected, hover) {
        (true, true, _) => 0.27,
        (true, _, true) => 0.18,
        (true, _, _) => 0.09,
        (false, true, _) => 0.90,
        (false, _, true) => 0.96,
        (false, _, _) => 1.0,
    }
}

fn paint_value(
    p: &mut Painter,
    s: &State,
    row: &Row,
    column: &Column,
    frame: NSRect,
    fg: &NSColor,
) {
    let NSPoint { x, y } = frame.origin;
    let value = if matches!(s.page, 0 | 4) && column.numeric {
        let cell = MetricCell::new(s, row, &column.key);
        if let Some(alpha) = cell.heat {
            let color = p.heat_colors[usize::from(alpha)].get_or_insert_with(|| {
                NSColor::colorWithSRGBRed_green_blue_alpha(
                    0.10,
                    0.58,
                    0.88,
                    f64::from(alpha) / 255.,
                )
            });
            color.setFill();
            NSBezierPath::fillRect(frame);
        }
        std::borrow::Cow::Owned(cell.text)
    } else {
        s.row_value(row, &column.key)
    };
    p.text(
        &value,
        [x + 8., y + 4., frame.size.width - 16., 22.],
        13.,
        false,
        fg,
        column.numeric,
    );
}

fn metric_cell_at(s: &State, area: NSRect) -> Option<(&Row, &Column, NSRect)> {
    let top = s.table_top();
    if area.origin.y < top || area.size.height > s.row_height() {
        return None;
    }
    let height = s.row_height();
    let middle_y = area.origin.y + area.size.height / 2.;
    let i = ((middle_y - top + s.scroll) / height).floor() as usize;
    let row = s.rows.get(i)?;
    if row.section {
        return None;
    }
    let sidebar = s.sidebar();
    let viewport = rect(sidebar, top, s.width - sidebar - 15., s.height - top - 15.);
    let y = top + i as f64 * height - s.scroll;
    let mut x = sidebar - s.hscroll;
    for column in &s.columns {
        let frame = rect(x, y, column.width, height);
        if column.numeric && NSIntersectionRect(viewport, frame) == area {
            return Some((row, column, frame));
        }
        x += column.width;
    }
    None
}

fn paint_metric_patch(p: &mut Painter, s: &State, area: NSRect) -> bool {
    let Some((row, column, frame)) = metric_cell_at(s, area) else {
        return false;
    };
    let selected = s.selected.as_ref() == Some(&row.id) || s.selections.contains(&row.id);
    let hover = matches!(&s.hover, Action::Select(id) if id == &row.id);
    gray(row_background(s.prefs.dark, selected, hover)).setFill();
    NSBezierPath::fillRect(area);
    let fg = gray(if s.prefs.dark { 0.94 } else { 0.08 });
    paint_value(p, s, row, column, frame, &fg);
    let edge = frame.origin.x + frame.size.width;
    if edge <= s.width - 15. {
        let rule = gray(if s.prefs.dark { 0.25 } else { 0.89 });
        p.line(
            &[(edge - 0.5, s.table_top()), (edge - 0.5, s.height - 15.)],
            &rule,
            0.65,
        );
    }
    true
}

pub fn redraw_table(p: &mut Painter, s: &mut State, view: &NSView, dirty: NSRect) {
    let metrics_only = matches!(s.page, 0 | 4)
        && dirty.origin.x >= s.metrics_left()
        && dirty.origin.x > s.sidebar()
        && dirty.origin.x + dirty.size.width <= s.width - 15.
        && dirty.origin.y + dirty.size.height <= s.height - 15.
        && !s.hits.is_empty();
    if !metrics_only {
        s.hits.retain(|hit| {
            !matches!(
                hit.action,
                Action::Sort(_)
                    | Action::ResizeColumn(_)
                    | Action::Select(_)
                    | Action::Expand(_)
                    | Action::ScrollH
                    | Action::ScrollV
                    | Action::ResetHistory
            )
        });
    }
    let needs = |r| view.needsToDrawRect(r);
    paint_table_region(p, s, metrics_only.then_some(&needs), dirty);
}

pub fn paint_table_region(
    p: &mut Painter,
    s: &mut State,
    damaged: Option<&dyn Fn(NSRect) -> bool>,
    dirty: NSRect,
) {
    if damaged.is_some() && paint_metric_patch(p, s, dirty) {
        return;
    }
    gray(if s.prefs.dark { 0.09 } else { 1. }).setFill();
    NSBezierPath::fillRect(dirty);
    table(p, s, damaged);
}

fn table(p: &mut Painter, s: &mut State, metrics_view: Option<&dyn Fn(NSRect) -> bool>) {
    let side = s.sidebar();
    let (w, h) = (s.width, s.height);
    let top = s.table_top();
    let rh = s.row_height();
    let hh = s.header_height();
    let dark = s.prefs.dark;
    let fg = gray(if dark { 0.94 } else { 0.08 });
    let muted = gray(if dark { 0.66 } else { 0.4 });
    let rule = gray(if dark { 0.25 } else { 0.89 });
    let mut x = side - s.hscroll;
    let content_w = w - side - 15.;
    unsafe {
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::clipRect(rect(side, 96., content_w, h - 111.));
    }
    if s.page == 2 {
        p.text(
            "Resource usage observed by Task Manager",
            [side + 16., 106., content_w - 30., 24.],
            11.,
            false,
            &muted,
            false,
        );
    }
    if s.page == 2 {
        p.text(
            "Delete usage history",
            [side + 16., 126., 180., 20.],
            11.,
            false,
            &rgb(0., 0.4, 0.64),
            false,
        );
        hit(
            s,
            [side + 16., 126., 180., 20.],
            Action::ResetHistory,
            "Delete usage history",
            true,
        );
    }
    for (column_index, c) in s.columns.clone().into_iter().enumerate() {
        if x > w {
            break;
        }
        if x + c.width < side {
            x += c.width;
            continue;
        }
        let r = [x, top - hh, c.width, hh];
        if metrics_view.is_some_and(|view| !c.numeric || !view(nsrect(r))) {
            x += c.width;
            continue;
        }
        let a = Action::Sort(c.key.clone());
        if s.sort == c.key || s.hover == a {
            let shade = if dark {
                0.22
            } else if s.hover == a {
                0.94
            } else {
                0.985
            };
            p.fill(r, &gray(shade), 0.);
        }
        if s.sort == c.key {
            let cx = x + c.width / 2.;
            let yy = top - hh + 5.;
            let pts = if s.ascending {
                [(cx - 3., yy + 3.), (cx, yy), (cx + 3., yy + 3.)]
            } else {
                [(cx - 3., yy), (cx, yy + 3.), (cx + 3., yy)]
            };
            p.line(&pts, &muted, 1.);
        }
        let summary = if [0, 4].contains(&s.page) {
            metric_heading(s, &c.key)
        } else {
            String::new()
        };
        if !summary.is_empty() {
            p.text(
                &summary,
                [x + 6., top - hh + 11., c.width - 14., 24.],
                if summary.len() > 9 { 11. } else { 16. },
                false,
                &fg,
                true,
            );
        }
        p.text(
            &c.title,
            [x + 8., top - 24., c.width - 16., 20.],
            13.,
            false,
            &*if c.numeric { muted.clone() } else { fg.clone() },
            c.numeric,
        );
        p.line(
            &[(x + c.width - 0.5, top - hh), (x + c.width - 0.5, top)],
            &rule,
            0.6,
        );
        if metrics_view.is_none() {
            hit(s, r, a, &format!("Sort by {}", c.title), true);
            hit(
                s,
                [x + c.width - 3., top - hh, 6., hh],
                Action::ResizeColumn(column_index),
                &format!("Resize {}", c.title),
                true,
            );
        }
        x += c.width;
    }
    p.line(&[(side, top - 0.5), (w, top - 0.5)], &rule, 1.);
    unsafe {
        NSGraphicsContext::restoreGraphicsState_class();
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::clipRect(rect(side, top, content_w, h - top - 15.));
    }
    for index in s.visible_rows() {
        let row = &s.rows[index];
        let y = top + index as f64 * rh - s.scroll;
        if metrics_view.is_some_and(|view| !view(rect(side, y, content_w, rh))) {
            continue;
        }
        if row.section {
            p.text(
                &row.name,
                [side + 16., y + 5., content_w - 25., 23.],
                15.,
                false,
                &fg,
                false,
            );
            continue;
        }
        let selected = s.selected.as_ref() == Some(&row.id) || s.selections.contains(&row.id);
        let hover = matches!(&s.hover, Action::Select(id) if id == &row.id);
        let hit_y = y.max(top);
        let hit_h = ((y + rh).min(h - 15.) - hit_y).max(0.);
        if metrics_view.is_none() {
            s.hits.push(Hit {
                rect: [side, hit_y, content_w, hit_h],
                action: Action::Select(row.id.clone()),
                label: row.name.clone(),
                enabled: true,
            });
        }
        if selected || hover {
            p.fill(
                [side, y, content_w, rh],
                &gray(row_background(dark, selected, hover)),
                0.,
            );
        }
        let mut x = side - s.hscroll;
        for c in &s.columns {
            if x > w {
                break;
            }
            if x + c.width < side
                || metrics_view.is_some_and(|v| !c.numeric || !v(rect(x, y, c.width, rh)))
            {
                x += c.width;
                continue;
            }
            if c.key == "name" {
                let indent = if row.indent { 20. } else { 0. };
                let mut tx = x + 12. + indent;
                if row.group {
                    let expanded = s.expanded.contains(&row.id);
                    p.icon(
                        if expanded { "down" } else { "right" },
                        tx,
                        y + 7.,
                        13.,
                        &muted,
                    );
                    if tx >= side && tx < side + content_w {
                        s.hits.push(Hit {
                            rect: [tx - 4., hit_y, 20., hit_h],
                            action: Action::Expand(row.id.clone()),
                            label: if expanded {
                                "Collapse group"
                            } else {
                                "Expand group"
                            }
                            .into(),
                            enabled: true,
                        });
                    }
                }
                if row.group || (s.page == 0 && s.prefs.grouped && !row.indent) {
                    tx += 21.;
                }
                if s.page == 6 {
                    p.icon("service.item", tx, y + 5., 17., &fg);
                } else if s.page == 4 && !row.indent {
                    p.icon("4", tx, y + 5., 17., &muted);
                } else if row.icon.ends_with(".app") {
                    p.image(&row.icon, [tx, y + 5., 17., 17.]);
                } else {
                    p.fill([tx + 1., y + 7., 15., 12.], &gray(0.97), 1.);
                    p.outline([tx + 1., y + 7., 15., 12.], &gray(0.52), 0.7, 1.);
                    p.fill([tx + 2., y + 8., 13., 2.5], &rgb(0.38, 0.64, 0.84), 0.);
                }
                tx += 25.;
                let name = if row.group && row.members.len() > 1 {
                    format!("{} ({})", row.name, row.members.len())
                } else {
                    row.name.clone()
                };
                p.text(
                    &name,
                    [tx, y + 4., (x + c.width - tx - 8.).max(0.), 22.],
                    13.,
                    false,
                    &fg,
                    false,
                );
            } else {
                paint_value(p, s, row, c, rect(x, y, c.width, rh), &fg);
            }
            x += c.width;
        }
    }
    let mut edge = side - s.hscroll;
    for c in &s.columns {
        edge += c.width;
        if edge > side && edge <= side + content_w {
            p.line(&[(edge - 0.5, top), (edge - 0.5, h - 15.)], &rule, 0.65);
        }
    }
    if s.rows.is_empty() {
        p.text(
            if s.query.is_empty() {
                "Collecting system data…"
            } else {
                "No matching results"
            },
            [side + 20., top + 28., content_w - 40., 30.],
            14.,
            false,
            &muted,
            false,
        );
    }
    unsafe {
        NSGraphicsContext::restoreGraphicsState_class();
    }
    if metrics_view.is_some() {
        return;
    }
    scrollbars(p, s, top, s.max_scroll(), s.scroll, false);
    let total: f64 = s.columns.iter().map(|c| c.width).sum();
    if total > content_w {
        p.fill(
            [side, h - 15., content_w, 15.],
            &gray(if dark { 0.14 } else { 0.965 }),
            0.,
        );
        let thumb = (content_w / total * content_w).max(30.);
        let tx = side + s.hscroll / (total - content_w) * (content_w - thumb);
        p.fill([tx + 1., h - 9., thumb - 2., 3.], &gray(0.53), 1.5);
        hit(
            s,
            [side, h - 15., content_w, 15.],
            Action::ScrollH,
            "Horizontal scrollbar",
            true,
        );
    }
}
fn scrollbars(p: &mut Painter, s: &mut State, top: f64, max: f64, offset: f64, settings: bool) {
    let viewport = s.height - top - 15.;
    let Some((thumb_top, thumb)) = vertical_scroll_thumb(viewport, max, offset) else {
        return;
    };
    let yy = top + thumb_top;
    p.fill(
        [s.width - 15., top, 15., viewport],
        &gray(if s.prefs.dark { 0.14 } else { 0.965 }),
        0.,
    );
    p.fill([s.width - 9., yy + 1., 3., thumb - 2.], &gray(0.53), 1.5);
    hit(
        s,
        [s.width - 15., top, 15., viewport],
        Action::ScrollV,
        if settings {
            "Settings scrollbar"
        } else {
            "Vertical scrollbar"
        },
        true,
    );
}
fn graph(
    p: &mut Painter,
    r: [f64; 4],
    values: &[(f64, Option<f64>)],
    secondary: &[(f64, Option<f64>)],
    max: f64,
    color: &NSColor,
    dark: bool,
    grid: bool,
) {
    let [x, y, w, h] = r;
    p.fill(r, &gray(if dark { 0.1 } else { 1. }), 0.);
    if grid {
        let lines = NSBezierPath::bezierPath();
        let mut xx = x;
        while xx < x + w {
            lines.moveToPoint(NSPoint::new(xx, y));
            lines.lineToPoint(NSPoint::new(xx, y + h));
            xx += 20.;
        }
        let mut yy = y;
        while yy < y + h {
            lines.moveToPoint(NSPoint::new(x, yy));
            lines.lineToPoint(NSPoint::new(x + w, yy));
            yy += 20.;
        }
        gray(if dark { 0.2 } else { 0.9 }).setStroke();
        lines.setLineWidth(0.5);
        lines.stroke();
    }
    p.outline(r, &color.colorWithAlphaComponent(0.6), 0.7, 0.);
    for (series, kernel) in [(values, false), (secondary, true)] {
        if series.len() < 2 {
            continue;
        }
        let path = NSBezierPath::bezierPath();
        let fill = NSBezierPath::bezierPath();
        for segment in series.split(|(_, v)| v.is_none()).filter(|v| !v.is_empty()) {
            let mut points: Vec<_> = segment
                .iter()
                .map(|&(position, value)| {
                    NSPoint::new(
                        x + position * w,
                        y + h - (value.unwrap() / max.max(1.)).clamp(0., 1.) * h,
                    )
                })
                .collect();
            let first = points[0];
            let end = points[points.len() - 1];
            let pointer = points.as_mut_ptr();
            path.moveToPoint(first);
            unsafe { path.appendBezierPathWithPoints_count(pointer, points.len() as isize) };
            if !kernel {
                fill.moveToPoint(NSPoint::new(first.x, y + h));
                unsafe { fill.appendBezierPathWithPoints_count(pointer, points.len() as isize) };
                fill.lineToPoint(NSPoint::new(end.x, y + h));
                fill.closePath();
            }
        }
        if !kernel {
            color
                .colorWithAlphaComponent(if dark { 0.17 } else { 0.075 })
                .setFill();
            fill.fill();
        }
        path.setLineWidth(if grid { 1.1 } else { 1. });
        if kernel {
            rgb(0.8, 0.3, 0.3).setStroke()
        } else {
            color.setStroke()
        }
        path.stroke();
    }
}
fn perf_values(
    s: &State,
    resource: usize,
    core: Option<usize>,
    kernel: bool,
) -> Vec<(f64, Option<f64>)> {
    let Some(last) = s.samples.back() else {
        return Vec::new();
    };
    let seconds = s.prefs.graph_seconds.clamp(30, 300) as f64;
    let first = s
        .samples
        .partition_point(|v| last.at.duration_since(v.at).as_secs_f64() > seconds);
    let mut values = Vec::with_capacity(s.samples.len() - first);
    for sample in s.samples.range(first..) {
        let x = 1. - last.at.duration_since(sample.at).as_secs_f64() / seconds;
        if !sample.continuous && !values.is_empty() {
            values.push((x, None));
        }
        let value = match resource {
            0 => match core {
                Some(i) => {
                    sample
                        .cores
                        .get(i)
                        .copied()
                        .flatten()
                        .map(|v| if kernel { v.1 } else { v.0 })
                }
                None if kernel => {
                    let mut cores = sample.cores.iter().flatten();
                    cores.next().map(|first| {
                        let (sum, count) =
                            cores.fold((first.1, 1), |(sum, n), c| (sum + c.1, n + 1));
                        sum / f64::from(count)
                    })
                }
                None => Some(sample.cpu),
            },
            1 => Some(sample.memory),
            2 => sample
                .disk
                .map(|(read, write)| if kernel { write } else { read + write }),
            3 => Some(if kernel {
                sample.sent
            } else {
                sample.received + sample.sent
            }),
            _ => None,
        };
        values.push((x, value));
    }
    values
}
fn performance(p: &mut Painter, s: &mut State) {
    let side = s.sidebar();
    let dark = s.prefs.dark;
    let fg = gray(if dark { 0.94 } else { 0.12 });
    let muted = gray(if dark { 0.65 } else { 0.45 });
    let colors = [
        rgb(0.30, 0.54, 0.62),
        rgb(0.61, 0.42, 0.66),
        rgb(0.48, 0.64, 0.27),
        rgb(0.66, 0.47, 0.27),
        rgb(0.49, 0.45, 0.63),
    ];
    let card_width = if s.graph_only { 0. } else { 200. };
    let top = if s.graph_only { 0. } else { 96. };
    let resource_names = ["CPU", "Memory", "Disk 0", "Network", "GPU 0"];
    if !s.graph_only {
        for (i, title) in resource_names.iter().enumerate() {
            let y = top + 12. + i as f64 * 76.;
            let a = Action::Resource(i);
            if s.resource == i || s.hover == a {
                p.fill(
                    [side + 8., y, card_width - 16., 68.],
                    &gray(if dark { 0.2 } else { 0.925 }),
                    0.,
                );
            }
            let values = perf_values(s, i, None, false);
            let max = if i == 2 || i == 3 {
                values.iter().filter_map(|v| v.1).fold(1., f64::max) * 1.1
            } else {
                100.
            };
            graph(
                p,
                [side + 18., y + 13., 60., 40.],
                &values,
                &[],
                max,
                &colors[i],
                dark,
                false,
            );
            p.text(
                title,
                [side + 90., y + 9., 102., 23.],
                12.,
                false,
                &fg,
                false,
            );
            let value = match i {
                0 => format!("{:0.0}%", s.system.cpu),
                1 => format!(
                    "{:0.1}/{:0.1} GB ({:0.0}%)",
                    s.system.used() as f64 / 1073741824.,
                    s.system.raw.ram as f64 / 1073741824.,
                    s.system.memory()
                ),
                2 => format!(
                    "{}/s",
                    bytes(s.system.read + s.system.write, s.prefs.binary)
                ),
                3 => format!(
                    "S: {}  R: {}",
                    bytes(s.system.sent, false),
                    bytes(s.system.received, false)
                ),
                _ => "–".into(),
            };
            p.text(
                &value,
                [side + 90., y + 34., 102., 20.],
                10.,
                false,
                &muted,
                false,
            );
            hit(s, [side + 8., y, card_width - 16., 68.], a, title, true);
        }
    }
    let left = side + card_width + if s.graph_only { 10. } else { 20. };
    let width = (s.width - left - 30.).max(180.);
    let gtop = top + if s.graph_only { 10. } else { 82. };
    let gh = if s.graph_only {
        s.height - 20.
    } else {
        (s.height - gtop - if s.resource == 1 { 300. } else { 235. }).max(100.)
    };
    let r = s.resource;
    let color = &colors[r];
    if !s.graph_only {
        p.text(
            resource_names[r],
            [left, top + 22., width * 0.5, 40.],
            24.,
            false,
            &fg,
            false,
        );
        let hardware = match r {
            0 => cstr(&s.system.raw.brand),
            1 => format!("{:0.1} GB", s.system.raw.ram as f64 / 1073741824.),
            2 => format!(
                "{} · {}",
                cstr(&s.system.raw.device),
                cstr(&s.system.raw.filesystem)
            ),
            3 => cstr(&s.system.raw.interfaces)
                .lines()
                .next()
                .unwrap_or("Network")
                .to_string(),
            _ => format!("{} GPU", cstr(&s.system.raw.brand)),
        };
        p.text(
            &hardware,
            [left + width * 0.38, top + 32., width * 0.62, 23.],
            12.,
            false,
            &fg,
            true,
        );
        p.text(
            match r {
                0 => "% Utilization",
                1 => "Memory usage",
                2 => "Disk transfer rate",
                3 => "Throughput",
                _ => "GPU utilization",
            },
            [left, gtop - 22., width * 0.7, 19.],
            10.,
            false,
            &muted,
            false,
        );
    }
    if r == 0 && s.prefs.logical {
        let n = s.system.raw.logical.max(1) as usize;
        let cols = if n <= 4 {
            2
        } else if n <= 12 {
            4
        } else {
            6
        };
        let rows = n.div_ceil(cols);
        let gw = (width - (cols - 1) as f64 * 8.) / cols as f64;
        let h = (gh - (rows - 1) as f64 * 8.) / rows as f64;
        for i in 0..n {
            let rr = [
                left + (i % cols) as f64 * (gw + 8.),
                gtop + (i / cols) as f64 * (h + 8.),
                gw,
                h,
            ];
            let v = perf_values(s, 0, Some(i), false);
            let k = if s.prefs.kernel {
                perf_values(s, 0, Some(i), true)
            } else {
                vec![]
            };
            graph(p, rr, &v, &k, 100., color, dark, true);
            p.text(
                &format!("CPU {i}"),
                [rr[0] + 5., rr[1] + 3., gw - 10., 18.],
                10.,
                false,
                &muted,
                false,
            );
            hit(
                s,
                rr,
                Action::Menu(format!("core:{i}")),
                &format!(
                    "CPU {i}: {:0.1}% utilization",
                    s.system
                        .cores
                        .get(i)
                        .and_then(|x| *x)
                        .map(|v| v.0)
                        .unwrap_or(0.)
                ),
                true,
            );
        }
    } else {
        let v = perf_values(s, r, None, false);
        let k = if r == 3 || r == 2 || (r == 0 && s.prefs.kernel) {
            perf_values(s, r, None, true)
        } else {
            vec![]
        };
        let max = if r == 2 || r == 3 {
            v.iter().filter_map(|v| v.1).fold(1., f64::max) * 1.1
        } else {
            100.
        };
        graph(p, [left, gtop, width, gh], &v, &k, max, color, dark, true);
        if r == 4 {
            p.text(
                "Utilization unavailable",
                [left + 15., gtop + 20., width - 30., 30.],
                13.,
                false,
                &muted,
                false,
            );
        }
        if !s.graph_only {
            p.text(
                &*if r < 2 {
                    "100%".into()
                } else {
                    format!("{}/s", bytes(max, s.prefs.binary))
                },
                [left + width - 150., gtop - 22., 150., 20.],
                10.,
                false,
                &muted,
                true,
            );
        }
        hit(
            s,
            [left, gtop, width, gh],
            Action::Menu("graph".into()),
            "Performance graph",
            true,
        );
    }
    if s.graph_only {
        return;
    }
    p.text(
        &format!("{} seconds", s.prefs.graph_seconds),
        [left, gtop + gh + 6., 150., 20.],
        10.,
        false,
        &muted,
        false,
    );
    p.text(
        "0",
        [left + width - 30., gtop + gh + 6., 30., 20.],
        10.,
        false,
        &muted,
        true,
    );
    let sy = gtop + gh + 40.;
    let mut metrics: Vec<(&str, String)> = vec![];
    let mut details: Vec<(&str, String)> = vec![];
    match r {
        0 => {
            metrics = vec![
                ("Utilization", format!("{:0.0}%", s.system.cpu)),
                ("Speed", "–".into()),
                ("Processes", s.records.len().to_string()),
                (
                    "Threads",
                    s.records
                        .iter()
                        .filter(|p| p.raw.valid != 0)
                        .try_fold(0u64, |sum, p| {
                            u64::try_from(p.raw.threads).ok().map(|n| sum + n)
                        })
                        .map(|sum| sum.to_string())
                        .unwrap_or_else(|| "–".into()),
                ),
                ("Handles", "–".into()),
                (
                    "Up time",
                    format!(
                        "{}:{}",
                        s.system.raw.uptime as u64 / 86400,
                        duration(s.system.raw.uptime % 86400.)
                    ),
                ),
            ];
            details = vec![
                ("Base speed:", "–".into()),
                ("Sockets:", "1".into()),
                ("Cores:", s.system.raw.physical.to_string()),
                ("Logical processors:", s.system.raw.logical.to_string()),
                ("Virtualization:", "Supported".into()),
                ("L1/L2/L3 cache:", "–".into()),
            ];
        }
        1 => {
            let used = s.system.used();
            let available = s.system.raw.ram.saturating_sub(used);
            p.text(
                "Memory composition",
                [left, sy - 8., width, 18.],
                10.,
                false,
                &muted,
                false,
            );
            let y = sy + 13.;
            p.outline([left, y, width, 25.], color, 0.8, 0.);
            p.fill(
                [left, y, width * s.system.memory() / 100., 25.],
                &color.colorWithAlphaComponent(0.3),
                0.,
            );
            metrics = vec![
                (
                    "In use (Compressed)",
                    format!(
                        "{} ({})",
                        bytes(used as f64, true),
                        bytes(s.system.raw.compressed as f64, true)
                    ),
                ),
                ("Available", bytes(available as f64, true)),
                (
                    "Committed",
                    format!(
                        "{} / {}",
                        bytes((used + s.system.raw.swap_used) as f64, true),
                        bytes((s.system.raw.ram + s.system.raw.swap_total) as f64, true)
                    ),
                ),
                ("Cached", bytes(s.system.raw.inactive as f64, true)),
                ("Wired", bytes(s.system.raw.wired as f64, true)),
                ("Swap used", bytes(s.system.raw.swap_used as f64, true)),
            ];
            details = vec![
                ("Speed:", "–".into()),
                ("Form factor:", "Unified memory".into()),
                ("Hardware reserved:", "–".into()),
            ];
        }
        2 => {
            metrics = vec![
                (
                    "Read speed",
                    format!("{}/s", bytes(s.system.read, s.prefs.binary)),
                ),
                (
                    "Write speed",
                    format!("{}/s", bytes(s.system.write, s.prefs.binary)),
                ),
                ("Active time", "–".into()),
                ("Average response time", "–".into()),
            ];
            details = vec![
                (
                    "Capacity:",
                    bytes(s.system.raw.capacity as f64, s.prefs.binary),
                ),
                (
                    "Available:",
                    bytes(s.system.raw.disk_free as f64, s.prefs.binary),
                ),
                ("System disk:", "Yes".into()),
                ("Type:", "SSD".into()),
            ];
        }
        3 => {
            metrics = vec![
                ("Send", format!("{}/s", bytes(s.system.sent, false))),
                ("Receive", format!("{}/s", bytes(s.system.received, false))),
            ];
            details = vec![
                ("Adapter:", "Physical interfaces".into()),
                (
                    "IPv4 address:",
                    cstr(&s.system.raw.interfaces).trim().into(),
                ),
            ];
        }
        _ => {
            metrics = vec![
                ("Utilization", "–".into()),
                ("GPU memory", "Unified".into()),
            ];
            details = vec![
                ("Graphics API:", "Metal".into()),
                ("Driver:", "macOS".into()),
            ];
        }
    }
    if !s.summary {
        let extra = if r == 1 { 52. } else { 0. };
        let available = width * 0.53;
        for (i, (title, value)) in metrics.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let (x, y) = if r == 0 {
                let positions = [
                    (0., 0.),
                    (available * 0.5, 0.),
                    (0., 55.),
                    (available / 3., 55.),
                    (available * 2. / 3., 55.),
                    (0., 110.),
                ];
                (left + positions[i].0, sy + positions[i].1)
            } else {
                (
                    left + col as f64 * available * 0.5,
                    sy + extra + row as f64 * 52.,
                )
            };
            let item_width = if r == 0 && (2..=4).contains(&i) {
                available / 3. - 8.
            } else if r == 0 && i == 5 {
                available
            } else {
                available * 0.5 - 10.
            };
            p.text(title, [x, y, item_width, 18.], 10., false, &muted, false);
            p.text(
                value,
                [x, y + 18., item_width, 30.],
                if value.len() > 16 { 13. } else { 20. },
                false,
                &fg,
                false,
            );
        }
        for (i, (key, value)) in details.iter().enumerate() {
            let x = left + width * 0.57;
            let y = sy + extra + i as f64 * 23.;
            p.text(key, [x, y, 125., 20.], 11., false, &muted, false);
            p.text(
                value,
                [x + 128., y, width * 0.43 - 128., 20.],
                11.,
                false,
                &fg,
                false,
            );
        }
    }
}
fn settings(p: &mut Painter, s: &mut State) {
    let left = s.sidebar() + 32.;
    let top = 76. - s.scroll;
    let width = s.width - left - 48.;
    let fg = gray(if s.prefs.dark { 0.94 } else { 0.12 });
    let muted = gray(if s.prefs.dark { 0.65 } else { 0.4 });
    unsafe {
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::clipRect(rect(
            s.sidebar(),
            48.,
            s.width - s.sidebar() - 15.,
            s.height - 48.,
        ));
    }
    p.text("Settings", [left, top, width, 42.], 24., true, &fg, false);
    let rows = [
        (
            "Default Start Page",
            PAGES[s.prefs.start].to_string(),
            "start",
        ),
        (
            "Real time update speed",
            match s.prefs.speed {
                0 => "Paused",
                1 => "High",
                4 => "Low",
                _ => "Normal",
            }
            .into(),
            "speed",
        ),
        ("Window management", String::new(), "group"),
        ("Always on top", s.prefs.top.to_string(), "top"),
        ("Minimize on use", s.prefs.minimize.to_string(), "minimize"),
        ("Hide when minimized", s.prefs.hide.to_string(), "hide"),
        ("Other options", String::new(), "group"),
        (
            "Show history for all processes",
            s.prefs.history_all.to_string(),
            "history-all",
        ),
        ("Launch at login", s.login_enabled.to_string(), "login"),
        (
            "Group application processes",
            s.prefs.grouped.to_string(),
            "grouped",
        ),
        (
            "App theme",
            if s.prefs.system_theme {
                "Use system setting"
            } else if s.prefs.dark {
                "Dark"
            } else {
                "Light"
            }
            .into(),
            "theme",
        ),
        (
            "Graph history",
            format!("{} seconds", s.prefs.graph_seconds),
            "history",
        ),
        (
            "Units",
            if s.prefs.binary { "Binary" } else { "Decimal" }.into(),
            "units",
        ),
    ];
    let mut y = top + 64.;
    for (title, value, key) in rows {
        if key == "group" {
            p.text(title, [left, y, width, 23.], 12., true, &fg, false);
            y += 34.;
            continue;
        }
        if value == "true" || value == "false" {
            let r = [left, y, 18., 18.];
            p.fill(
                r,
                &*if value == "true" {
                    rgb(0.0, 0.45, 0.68)
                } else {
                    gray(if s.prefs.dark { 0.15 } else { 0.98 })
                },
                3.,
            );
            p.outline(r, &muted, 0.8, 3.);
            if value == "true" {
                p.icon("check", left + 1., y + 1., 16., &gray(1.));
            }
            p.text(
                title,
                [left + 28., y - 1., width - 28., 23.],
                12.,
                false,
                &fg,
                false,
            );
            hit(
                s,
                [left, y - 4., width, 26.],
                Action::Setting(key.into()),
                title,
                true,
            );
            y += 34.;
        } else {
            p.text(title, [left, y, width, 22.], 12., true, &fg, false);
            y += 29.;
            p.fill(
                [left, y, 230., 30.],
                &gray(if s.prefs.dark { 0.18 } else { 0.99 }),
                3.,
            );
            p.outline(
                [left, y, 230., 30.],
                &gray(if s.prefs.dark { 0.32 } else { 0.85 }),
                0.8,
                3.,
            );
            p.text(
                &value,
                [left + 10., y + 6., 190., 22.],
                12.,
                false,
                &fg,
                false,
            );
            p.icon("down", left + 205., y + 10., 12., &fg);
            hit(
                s,
                [left, y, 230., 30.],
                Action::Setting(key.into()),
                title,
                true,
            );
            y += 59.;
        }
    }
    p.text(
        concat!(
            "Task Manager ",
            env!("CARGO_PKG_VERSION"),
            " · Native Rust + AppKit"
        ),
        [left, y + 6., width, 24.],
        12.,
        false,
        &muted,
        false,
    );
    control(
        p,
        s,
        [left, y + 36., 170., 34.],
        "About Task Manager",
        "",
        Action::About,
        true,
    );
    unsafe {
        NSGraphicsContext::restoreGraphicsState_class();
    }
    s.settings_height = y + 90. + s.scroll;
    scrollbars(
        p,
        s,
        48.,
        (y + 90. + s.scroll - s.height).max(0.),
        s.scroll,
        true,
    );
}
fn menus(p: &mut Painter, s: &mut State) {
    let fg = gray(if s.prefs.dark { 0.94 } else { 0.08 });
    let muted = gray(if s.prefs.dark { 0.45 } else { 0.62 });
    for depth in 0..s.menus.len() {
        let menu = &s.menus[depth];
        let x = menu.x;
        let y = menu.y;
        let items = menu.items.clone();
        let selected = menu.selected;
        let height: f64 = items
            .iter()
            .map(|v| if v.title.is_empty() { 9. } else { 30. })
            .sum::<f64>()
            + 8.;
        let width = 252.;
        p.fill(
            [x + 2., y + 3., width, height],
            &gray(if s.prefs.dark { 0.04 } else { 0.78 }),
            5.,
        );
        p.fill(
            [x, y, width, height],
            &gray(if s.prefs.dark { 0.17 } else { 0.975 }),
            5.,
        );
        p.outline(
            [x, y, width, height],
            &gray(if s.prefs.dark { 0.30 } else { 0.86 }),
            0.8,
            5.,
        );
        let mut yy = y + 4.;
        for (i, item) in items.iter().enumerate() {
            if item.title.is_empty() {
                p.line(
                    &[(x + 5., yy + 4.), (x + width - 5., yy + 4.)],
                    &gray(if s.prefs.dark { 0.28 } else { 0.88 }),
                    0.7,
                );
                yy += 9.;
                continue;
            }
            if selected == Some(i) && item.enabled {
                p.fill(
                    [x + 4., yy, width - 8., 29.],
                    &gray(if s.prefs.dark { 0.26 } else { 0.92 }),
                    2.,
                );
            }
            let color = if item.enabled { &fg } else { &muted };
            if item.checked {
                p.icon("check", x + 10., yy + 9., 12., color)
            }
            p.text(
                &item.title,
                [x + 30., yy + 6., width - 50., 23.],
                12.,
                false,
                color,
                false,
            );
            if !item.children.is_empty() {
                p.icon("right", x + width - 22., yy + 9., 12., color)
            }
            hit(
                s,
                [x + 4., yy, width - 8., 30.],
                Action::Option(format!("menu:{depth}:{i}")),
                &item.title,
                item.enabled,
            );
            yy += 30.;
        }
    }
}
fn dialog(p: &mut Painter, s: &mut State) {
    let Some(d) = &s.dialog else { return };
    let title = d.title.clone();
    let message = d.message.clone();
    let ok = d.ok.clone();
    let input = d.input;
    let fg = gray(if s.prefs.dark { 0.94 } else { 0.1 });
    p.fill(
        [0., 0., s.width, s.height],
        &gray(0.).colorWithAlphaComponent(0.18),
        0.,
    );
    let inspector = title.ends_with(" Properties");
    let w = if inspector {
        (s.width - 80.).min(760.)
    } else {
        480.
    };
    let h = if input {
        224.
    } else if inspector {
        (s.height - 100.).min(580.)
    } else {
        280.
    };
    let x = (s.width - w) / 2.;
    let y = (s.height - h) / 2.;
    p.fill(
        [x, y, w, h],
        &gray(if s.prefs.dark { 0.16 } else { 0.985 }),
        8.,
    );
    p.outline([x, y, w, h], &gray(0.65), 0.7, 8.);
    p.text(
        &title,
        [x + 24., y + 22., w - 48., 30.],
        18.,
        true,
        &fg,
        false,
    );
    let mut lines = Vec::new();
    let columns = if inspector {
        ((w - 52.) / 6.2) as usize
    } else {
        68
    };
    for line in message.lines() {
        let chars = line.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            lines.push(String::new());
        }
        for part in chars.chunks(columns) {
            lines.push(part.iter().collect::<String>());
        }
    }
    let available = h - 126.;
    let maximum = (lines.len() as f64 * 20. - available).max(0.);
    s.dialog_scroll = s.dialog_scroll.min(maximum);
    unsafe {
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::clipRect(rect(x + 24., y + 62., w - 48., available));
    }
    for (i, line) in lines.iter().enumerate() {
        p.text(
            line,
            [
                x + 24.,
                y + 62. + i as f64 * 20. - s.dialog_scroll,
                w - 48.,
                22.,
            ],
            12.,
            false,
            &fg,
            false,
        );
    }
    unsafe {
        NSGraphicsContext::restoreGraphicsState_class();
    }
    if inspector {
        control(
            p,
            s,
            [x + 24., y + h - 50., 125., 32.],
            "Copy details",
            "",
            Action::Option("copy-properties".into()),
            true,
        );
    }
    let by = y + h - 50.;
    let r = [x + w - 230., by, 96., 32.];
    p.fill(r, &rgb(0.0, 0.42, 0.65), 3.);
    p.text(
        &ok,
        [r[0] + 10., by + 7., 76., 22.],
        12.,
        false,
        &gray(1.),
        false,
    );
    hit(s, r, Action::DialogOk, &ok, true);
    control(
        p,
        s,
        [x + w - 120., by, 96., 32.],
        "Cancel",
        "",
        Action::DialogCancel,
        true,
    );
}

#[cfg(test)]
mod patch_geometry_tests {
    use super::*;

    #[test]
    fn cell_patch_uses_unclipped_coordinates_while_scrolled() {
        let mut state = State::new();
        state.page = 0;
        state.columns = columns(0);
        state.rows = vec![Row {
            id: "visible".into(),
            ..Row::default()
        }];
        state.hscroll = 470.;
        state.scroll = 6.;
        let side = state.sidebar();
        let top = state.table_top();
        let area = rect(side, top, 35., 22.);
        let (row, column, frame) = metric_cell_at(&state, area).unwrap();
        assert_eq!(row.id, "visible");
        assert_eq!(column.key, "cpu");
        assert_eq!(frame, rect(side - 50., top - 6., 85., 28.));
        assert!(metric_cell_at(&state, rect(side, top, 36., 22.)).is_none());
        state.rows[0].section = true;
        assert!(metric_cell_at(&state, area).is_none());
    }
}
