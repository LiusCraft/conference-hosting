use gpui::{px, svg, Rgba, Styled, Svg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Activity,
    AudioLines,
    Bot,
    Cable,
    ChevronDown,
    Clock,
    FileCode,
    Globe,
    Headphones,
    Info,
    Mic,
    MicOff,
    MonitorSpeaker,
    Send,
    Server,
    Settings,
    ShieldCheck,
    User,
    Volume2,
    VolumeX,
    Wifi,
    WifiOff,
    Wrench,
    X,
    Zap,
}

impl IconName {
    const fn path(self) -> &'static str {
        match self {
            Self::Activity => "svg/activity.svg",
            Self::AudioLines => "svg/audio-lines.svg",
            Self::Bot => "svg/bot.svg",
            Self::Cable => "svg/cable.svg",
            Self::ChevronDown => "svg/chevron-down.svg",
            Self::Clock => "svg/clock.svg",
            Self::FileCode => "svg/file-code.svg",
            Self::Globe => "svg/globe.svg",
            Self::Headphones => "svg/headphones.svg",
            Self::Info => "svg/info.svg",
            Self::Mic => "svg/mic.svg",
            Self::MicOff => "svg/mic-off.svg",
            Self::MonitorSpeaker => "svg/monitor-speaker.svg",
            Self::Send => "svg/send.svg",
            Self::Server => "svg/server.svg",
            Self::Settings => "svg/settings.svg",
            Self::ShieldCheck => "svg/shield-check.svg",
            Self::User => "svg/user.svg",
            Self::Volume2 => "svg/volume-2.svg",
            Self::VolumeX => "svg/volume-x.svg",
            Self::Wifi => "svg/wifi.svg",
            Self::WifiOff => "svg/wifi-off.svg",
            Self::Wrench => "svg/wrench.svg",
            Self::X => "svg/x.svg",
            Self::Zap => "svg/zap.svg",
        }
    }
}

pub fn icon(name: IconName, size_px: f32, color: Rgba) -> Svg {
    svg().path(name.path()).size(px(size_px)).text_color(color)
}
