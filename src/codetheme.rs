//! Editor themes. The values are intentionally kept as a flat nine-color
//! palette so they can also be exported to Monaco / CodeMirror later.

use eframe::egui::Color32;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeTheme {
    pub name: &'static str,
    pub dark: bool,
    pub bg: Color32,
    pub text: Color32,
    pub comment: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub function: Color32,
    pub selection: Color32,
    pub line_highlight: Color32,
}

impl CodeTheme {
    /// Stable cache key for layout/invalidation. Do not reorder names casually.
    pub fn key(self) -> u64 {
        self.name.bytes().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }
}

macro_rules! theme {
    ($name:literal, $dark:literal, $bg:literal, $text:literal, $comment:literal, $keyword:literal, $string:literal, $number:literal, $function:literal, $selection:literal, $line:literal) => {
        CodeTheme {
            name: $name,
            dark: $dark,
            bg: rgb($bg),
            text: rgb($text),
            comment: rgb($comment),
            keyword: rgb($keyword),
            string: rgb($string),
            number: rgb($number),
            function: rgb($function),
            selection: rgb($selection),
            line_highlight: rgb($line),
        }
    };
}

pub static THEMES: &[CodeTheme] = &[
    theme!(
        "示波器原色",
        true,
        0x121719,
        0xc5d2d6,
        0x54656b,
        0x62b7d0,
        0xd49a6a,
        0x89c47e,
        0x72a8d8,
        0x243b46,
        0x182024
    ),
    theme!(
        "焊台琥珀",
        true,
        0x1a1714,
        0xd9cbc0,
        0x6b5f56,
        0xd8905b,
        0x88a880,
        0xb88b9e,
        0x94aed1,
        0x382b20,
        0x241f1a
    ),
    theme!(
        "CRT 磷绿",
        true,
        0x0a100a,
        0x73d473,
        0x387038,
        0x9be69b,
        0xbce9bc,
        0x8ed98e,
        0xa5e8a5,
        0x1c361c,
        0x101a10
    ),
    theme!(
        "机箱炭黑",
        true,
        0x111315,
        0xabb4ba,
        0x4a545a,
        0x789fb8,
        0xb99278,
        0x88a999,
        0x92a8c2,
        0x202830,
        0x16191c
    ),
    theme!(
        "伯德暗",
        true,
        0x10141b,
        0xc2cad8,
        0x4c5870,
        0x709cdb,
        0xc79274,
        0x76b89e,
        0x89a8d6,
        0x222d42,
        0x161b26
    ),
    theme!(
        "锈蚀面板",
        true,
        0x1b1513,
        0xcbb8ad,
        0x705c54,
        0xc97858,
        0x749980,
        0x998899,
        0x849fc2,
        0x3b2720,
        0x271e1a
    ),
    theme!(
        "钴蓝控制台",
        true,
        0x0f141d,
        0xb9c7dd,
        0x465673,
        0x6398e8,
        0xd4947c,
        0x72c2a8,
        0x82a9e5,
        0x1f2d47,
        0x141a27
    ),
    theme!(
        "灰度控制台",
        true,
        0x171717,
        0xbbbbbb,
        0x666666,
        0x999999,
        0xaaaaaa,
        0x888888,
        0xb0b0b0,
        0x2c2c2c,
        0x1e1e1e
    ),
    theme!(
        "95 文档绿",
        true,
        0x101814,
        0xb4c9bf,
        0x486659,
        0x72b899,
        0xc9947c,
        0x88c7aa,
        0x82b8c2,
        0x20352b,
        0x151d18
    ),
    theme!(
        "暗房读数",
        true,
        0x141211,
        0xc7b9b2,
        0x685c56,
        0xd08468,
        0x8299b8,
        0xb88899,
        0x92a8c2,
        0x2f2420,
        0x1a1715
    ),
    theme!(
        "基线蓝",
        true,
        0x11161c,
        0xc0cadd,
        0x44556b,
        0x68a0e0,
        0xd49074,
        0x70b8a4,
        0x84a8dd,
        0x202c40,
        0x151b23
    ),
    theme!(
        "采样灰",
        true,
        0x16181a,
        0xb8c0c8,
        0x505860,
        0x7498b8,
        0xc49478,
        0x80ac9c,
        0x88aacc,
        0x283038,
        0x1b1e21
    ),
    theme!(
        "频域暗",
        true,
        0x0f131a,
        0xc1c9da,
        0x424e66,
        0x749ee2,
        0xd89676,
        0x72b2aa,
        0x86aadd,
        0x1e293d,
        0x141820
    ),
    theme!(
        "差分", true, 0x131518, 0xbdc5ce, 0x4c545c, 0x78a2c2, 0xcc9276, 0x82b0a0, 0x8aaed0,
        0x262e38, 0x181b1f
    ),
    theme!(
        "阻抗", true, 0x121617, 0xc0c8ca, 0x4e585b, 0x68a8b8, 0xd4987a, 0x86b8a6, 0x7eb8cc,
        0x222d30, 0x171b1c
    ),
    theme!(
        "信号舱",
        true,
        0x0e1218,
        0xc2cbe0,
        0x404c62,
        0x78a6e8,
        0xdd9478,
        0x74b8ac,
        0x88ace8,
        0x1c283e,
        0x13171e
    ),
    theme!(
        "总线暗",
        true,
        0x141419,
        0xc0c0cf,
        0x505066,
        0x828ed8,
        0xd8987c,
        0x88b4aa,
        0x94a2dd,
        0x262638,
        0x19191f
    ),
    theme!(
        "探针", true, 0x111516, 0xbfc7c9, 0x4a5456, 0x66b0b8, 0xd49678, 0x84bca8, 0x7ab8c8,
        0x212b2d, 0x161a1b
    ),
    theme!(
        "隔离舱",
        true,
        0x101317,
        0xc1c8d5,
        0x444e5e,
        0x7096d0,
        0xd29074,
        0x78b4a2,
        0x82a4d2,
        0x1f2936,
        0x14171b
    ),
    theme!(
        "时序窗",
        true,
        0x121216,
        0xc4c4d2,
        0x4e4e60,
        0x848ed4,
        0xda9a7e,
        0x8ab8ac,
        0x96a4d4,
        0x242434,
        0x17171b
    ),
    theme!(
        "铜迹", true, 0x181411, 0xd2c2b6, 0x64564b, 0xd48c66, 0x82a292, 0xb28896, 0x8ca8c8,
        0x342820, 0x201a16
    ),
    theme!(
        "焊斑", true, 0x191613, 0xcdc0b4, 0x62584e, 0xcc8862, 0x86a694, 0xb08a94, 0x88a4c4,
        0x322a22, 0x1f1b18
    ),
    theme!(
        "沥青控制台",
        true,
        0x0f0f0f,
        0xb5b5b5,
        0x585858,
        0x888888,
        0xb09078,
        0x80a090,
        0x90a0b8,
        0x222222,
        0x141414
    ),
    theme!(
        "工装板",
        true,
        0x151514,
        0xc2c0bc,
        0x54524e,
        0x94928c,
        0xd09476,
        0x88aa98,
        0x98acc0,
        0x282724,
        0x1a1a18
    ),
    theme!(
        "暗页岩",
        true,
        0x141618,
        0xb9c2c8,
        0x4c545a,
        0x7296b4,
        0xca9074,
        0x7ead9c,
        0x84a6c8,
        0x242c32,
        0x191c1e
    ),
    theme!(
        "青岩暗",
        true,
        0x111717,
        0xb8c6c6,
        0x445c5c,
        0x62a8a8,
        0xd49276,
        0x78bcac,
        0x7ab8c4,
        0x203030,
        0x151b1b
    ),
    theme!(
        "沉木暗",
        true,
        0x171412,
        0xcbbfb6,
        0x60544c,
        0xc88660,
        0x84a494,
        0xb08892,
        0x8aa6c6,
        0x302620,
        0x1e1a17
    ),
    theme!(
        "雾岩暗",
        true,
        0x131619,
        0xbcc6cf,
        0x485460,
        0x7098c0,
        0xcc9274,
        0x7eb0a0,
        0x86a8cc,
        0x222c36,
        0x181b1e
    ),
    theme!(
        "松烟", true, 0x121315, 0xc2c5c9, 0x4c5056, 0x788eb8, 0xd09476, 0x84aa9a, 0x8eaad0,
        0x242830, 0x17191b
    ),
    theme!(
        "苔原暗",
        true,
        0x101613,
        0xb8c8c0,
        0x425c50,
        0x68b08e,
        0xd49478,
        0x86c2a6,
        0x80b4b8,
        0x1e3028,
        0x141a17
    ),
    theme!(
        "晚岩", true, 0x161417, 0xc4bfc8, 0x58505e, 0xa078c2, 0xd49276, 0x94b2a4, 0xa098d0,
        0x2c2432, 0x1b181d
    ),
    theme!(
        "雾川暗",
        true,
        0x111519,
        0xbcd0e0,
        0x445468,
        0x68a8e0,
        0xd49478,
        0x78b8ac,
        0x82b0e0,
        0x202c3c,
        0x15191d
    ),
    theme!(
        "校准台黑",
        true,
        0x101213,
        0xc0c5c8,
        0x484f53,
        0x7090a8,
        0xcc9072,
        0x7eb09e,
        0x82a0c2,
        0x20262a,
        0x141617
    ),
    theme!(
        "旧示波器蓝",
        true,
        0x0f141a,
        0xb9c8dd,
        0x42546b,
        0x6094dd,
        0xd29074,
        0x70b6a6,
        0x80a8dd,
        0x1e2a3e,
        0x131820
    ),
    theme!(
        "老式串口黑",
        true,
        0x0e1113,
        0xbdc4c9,
        0x404a50,
        0x6892b0,
        0xce9072,
        0x78ae9c,
        0x80a2c4,
        0x1c242a,
        0x121416
    ),
    theme!(
        "电源暗",
        true,
        0x151311,
        0xd0c4b8,
        0x5e544a,
        0xc48460,
        0x82a090,
        0xae8690,
        0x86a2c2,
        0x30261e,
        0x1d1a17
    ),
    theme!(
        "FOC 暗", true, 0x101518, 0xc0cad2, 0x44525c, 0x68a0bc, 0xd09274, 0x78b4a4, 0x80a8c8,
        0x202a32, 0x14181b
    ),
    theme!(
        "双脉冲暗",
        true,
        0x111416,
        0xbec6cc,
        0x465056,
        0x6c96b4,
        0xd29274,
        0x7ab0a0,
        0x82a4c4,
        0x222a30,
        0x151719
    ),
    theme!(
        "EMI 暗", true, 0x121418, 0xc0c6d2, 0x464e5e, 0x7094c2, 0xd09072, 0x78b0a2, 0x82a6c8,
        0x222836, 0x16181c
    ),
    theme!(
        "接地暗",
        true,
        0x131315,
        0xc4c4c8,
        0x4e4e54,
        0x8088a8,
        0xd29476,
        0x84aa9a,
        0x90aacc,
        0x26262c,
        0x18181a
    ),
    theme!(
        "硫酸纸",
        false,
        0xf2efe9,
        0x2c333a,
        0x7c848c,
        0x346899,
        0x995c3b,
        0x48876c,
        0x5078a8,
        0xd6e2f0,
        0xe9e5dd
    ),
    theme!(
        "云母白",
        false,
        0xf7f7f5,
        0x24272b,
        0x82888f,
        0x4068a3,
        0xa05c42,
        0x498872,
        0x5a7cb3,
        0xdce6f2,
        0xefefe9
    ),
    theme!(
        "米黄工程纸",
        false,
        0xf3efe4,
        0x2d3438,
        0x7a8286,
        0x386b9b,
        0x9b5e3d,
        0x4a896e,
        0x547aa9,
        0xdde5ee,
        0xebe7dd
    ),
    theme!(
        "日光校准",
        false,
        0xf5f3ee,
        0x2a3034,
        0x767e82,
        0x366997,
        0x985b3a,
        0x47866b,
        0x5279a7,
        0xdae4ed,
        0xedeae3
    ),
    theme!(
        "浅岩", false, 0xf4f4f1, 0x282c30, 0x788084, 0x3c6da0, 0x9c5f3e, 0x4b8a6f, 0x567bab,
        0xdbe5ef, 0xedece8
    ),
    theme!(
        "青灰笺",
        false,
        0xf0f3f5,
        0x2b3238,
        0x747c84,
        0x3a6fa2,
        0x9a5d3c,
        0x49886d,
        0x547cac,
        0xd7e3ee,
        0xe8ecf0
    ),
    theme!(
        "象牙稿",
        false,
        0xf6f2e9,
        0x2c3135,
        0x798185,
        0x376a98,
        0x995c3b,
        0x48876c,
        0x537aa8,
        0xdde4ed,
        0xefeae1
    ),
    theme!(
        "浅校准灰",
        false,
        0xf1f2f3,
        0x292e32,
        0x757d81,
        0x3b6c9a,
        0x9a5d3c,
        0x49886d,
        0x557ba9,
        0xd9e3ee,
        0xe9ebec
    ),
    theme!(
        "打印稿",
        false,
        0xf8f6f2,
        0x272b2f,
        0x727a7e,
        0x356896,
        0x975a39,
        0x46856a,
        0x5178a6,
        0xe0e7f0,
        0xf0ede8
    ),
    theme!(
        "日光示波器",
        false,
        0xedf1f3,
        0x2a3339,
        0x707c82,
        0x32709c,
        0x965a38,
        0x448468,
        0x5079a8,
        0xd4e2ec,
        0xe6ebed
    ),
];

pub fn by_name(name: &str) -> Option<&'static CodeTheme> {
    THEMES.iter().find(|theme| theme.name == name)
}

pub fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

pub fn default_theme() -> &'static CodeTheme {
    &THEMES[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_all_fifty_themes() {
        assert_eq!(THEMES.len(), 50);
        assert_eq!(THEMES.iter().filter(|theme| theme.dark).count(), 40);
        assert_eq!(THEMES.iter().filter(|theme| !theme.dark).count(), 10);
        let mut names = THEMES.iter().map(|theme| theme.name).collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 50);
        assert_eq!(THEMES[39].name, "接地暗");
        assert!(THEMES[39].dark);
        assert_eq!(THEMES[40].name, "硫酸纸");
        assert!(!THEMES[40].dark);
        assert_eq!(default_theme().name, "示波器原色");
        assert_eq!(
            by_name("日光示波器").map(|theme| theme.bg),
            Some(rgb(0xedf1f3))
        );
    }
}
