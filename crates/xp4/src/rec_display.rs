use fujimod_emulator_cpu::arm;
use fujimod_emulator_xp4_board::Session;
use serde_json::{Value, json};

pub const SOURCE_SHA256: &str = "e5efc48eeb3b4a9ff97a78b46833a4dcbd23a1682f0fc2476d8a8f12a4a3c4f8";
pub const COMPOSITION_SOURCE_SHA256: &str =
    "5fadbac30876d092462f85c9b955095b1f4231e76ca664638dfdc738b2e05838";
pub const FLOW_SOURCE_SHA256: &str =
    "c4e8ca5bef3cdb31f16848049e2df507ab9d9d5c3971d902af612e44e803528e";
pub const CONTINUATION_SOURCE_SHA256: &str =
    "dbf7fafe0aa9fcb677addfc40ffc37a0a0e783dd3dfe0f7bcd6bf0812d9ef8cd";
pub const LIMIT: usize = 16384;
pub const POINTS: [u64; 198] = [
    0x122e66c, 0x12310cc, 0x5a9f8, 0x121a3b8, 0x122c814, 0x122c838, 0x1246d4c, 0x124688c,
    0x1246974, 0x1248930, 0x1248974, 0x124871c, 0x1248730, 0x1246a1c, 0x1246a40, 0x1246a5c,
    0x1246a78, 0x1246ac8, 0x1246af8, 0x1246a88, 0x1246ad8, 0x1246b08, 0x1246958, 0x12466d0,
    0x12466e0, 0x12466f4, 0x1247120, 0x124bc88, 0x1248844, 0x1248888, 0x12488bc, 0x1248908,
    0x124bdb4, 0x1248860, 0x124bd90, 0x2177a18, 0x2177c88, 0x2177fd8, 0x21980f0, 0x121a36c,
    0x122c868, 0x122c888, 0x122c89c, 0x122c8a4, 0x219813c, 0x219817c, 0x2198190, 0x1218f04,
    0x121e0f4, 0x1219074, 0x12190b0, 0x1219124, 0x1219244, 0x12191dc, 0x12198cc, 0x121e13c,
    0x12191e0, 0x12198d0, 0x121de40, 0x219818c, 0x1218f38, 0x21821e0, 0x2181fc0, 0x212d010,
    0x2120edc, 0x2120ef8, 0x21822f8, 0x2182348, 0x218236c, 0x212cff8, 0x2120efc, 0x2120f3c,
    0x216d768, 0x212cfc4, 0x216d7d0, 0x216d7d4, 0x2197f58, 0x121b3a8, 0x121b460, 0x121b464,
    0x2197f7c, 0x216d74c, 0x2182140, 0x12358fc, 0x1235908, 0x12373d0, 0x12373e0, 0x1236f44,
    0x1236f50, 0x12370d4, 0x12370e8, 0x12370ec, 0x123720c, 0x1237254, 0x1237258, 0x1236c6c,
    0x2181c50, 0x2181c88, 0x2181ca8, 0x2181cc4, 0x2181ccc, 0x2152ba0, 0x2152bbc, 0x2152bd8,
    0x21807c4, 0x21807e4, 0x2181ef0, 0x218077c, 0x2181530, 0x21818d0, 0x2180ae4, 0x2180b64,
    0x2181254, 0x2180fe8, 0x2180728, 0x218075c, 0x21814ec, 0x218151c, 0x21818bc, 0x2180abc,
    0x2180b14, 0x2180b3c, 0x2180d0c, 0x2181220, 0x2181ed8, 0x21539e0, 0x21539fc, 0x2153a08,
    0x2153a28, 0x2153a38, 0x2170c78, 0x2153a54, 0x2153ab4, 0x2153a10, 0x2173ac8, 0x2173ae4,
    0x2173ae8, 0x212a190, 0x2153058, 0x2153180, 0x21539b0, 0x2153b94, 0x2172f78, 0x212a008,
    0x2172e64, 0x21cb6fc, 0x21cb728, 0x21cb770, 0x21f0a24, 0x21cb7b8, 0x21cb7c0, 0x21805ec,
    0x2180248, 0x2180278, 0x2180284, 0x21f09d0, 0x218028c, 0x21cded4, 0x21f1f84, 0x21f1f98,
    0x21f1f9c, 0x21cfcc4, 0x21ce55c, 0x21ce65c, 0x21e8e10, 0x223d338, 0x21cff6c, 0x21d67ec,
    0x2256460, 0x22521f0, 0x2245fcc, 0x2246008, 0x223dd68, 0x223dd78, 0x223dda0, 0x223dd8c,
    0x223ddb4, 0x2251560, 0x2251624, 0x2251630, 0x2251658, 0x2246660, 0x2252274, 0x225339c,
    0x225340c, 0x2251534, 0x224577c, 0x22457a4, 0x2249258, 0x22493dc, 0x22493e0, 0x2249434,
    0x2249438, 0x2249448, 0x22491d8, 0x223f530, 0x223f500, 0x223f504,
];
const RANGES: [(usize, usize); 7] = [
    (0x5a9f0, 0x5aa08),
    (0x121a3b8, 0x121a3f4),
    (0x122c814, 0x122c844),
    (0x1217ed0, 0x1217f34),
    (0x122e66c, 0x122e6ec),
    (0x122e820, 0x122e8f0),
    (0x1231054, 0x12310d0),
];
const COMPOSITION_RANGES: [(usize, usize); 9] = [
    (0x124688c, 0x1246a1c),
    (0x1246a1c, 0x1246b34),
    (0x1248930, 0x12489e8),
    (0x12486e8, 0x1248790),
    (0x1246d4c, 0x1246eb4),
    (0x12472d0, 0x1247330),
    (0x1247420, 0x12474e0),
    (0x1247530, 0x12475d4),
    (0x1246658, 0x1246674),
];

const FLOW_RANGES: [(usize, usize); 16] = [
    (0x1247120, 0x1247620),
    (0x1246674, 0x124672c),
    (0x1606520, 0x1606580),
    (0x1248abc, 0x1248af0),
    (0x12487fc, 0x1248930),
    (0x124bc70, 0x124bd90),
    (0x124bd90, 0x124be50),
    (0x1248560, 0x1248608),
    (0x1248674, 0x12486e8),
    (0x12484e8, 0x124850c),
    (0x1248abc, 0x1248af0),
    (0x121a36c, 0x121a3b8),
    (0x122c868, 0x122c8b8),
    (0x21980f0, 0x219813c),
    (0x21779dc, 0x2177b10),
    (0x2177c88, 0x2177ffc),
];

const CONTINUATION_RANGES: [(usize, usize); 319] = [
    (0x219813c, 0x21981a4),
    (0x21980b0, 0x219813c),
    (0x2197e90, 0x2197ed0),
    (0x2197ad4, 0x2197e90),
    (0x2197a58, 0x2197ad4),
    (0x1218ee8, 0x1218f50),
    (0x121de08, 0x121de4c),
    (0x121dc24, 0x121dc58),
    (0x121dc58, 0x121dce4),
    (0x121e098, 0x121e21c),
    (0x1219074, 0x1219918),
    (0x121ae68, 0x121ae94),
    (0x123349c, 0x1233550),
    (0x127e784, 0x127e914),
    (0x127e9d0, 0x127ea8c),
    (0x2181fc0, 0x2181fdc),
    (0x2181fc0, 0x2181fdc),
    (0x21821c4, 0x21821e8),
    (0x2182124, 0x2182160),
    (0x215df20, 0x215df38),
    (0x215dff4, 0x215e050),
    (0x215e050, 0x215e064),
    (0x2162370, 0x21623c0),
    (0x2129958, 0x21299c8),
    (0x2129b94, 0x2129bb0),
    (0x2129b34, 0x2129b50),
    (0x212cfb0, 0x212d104),
    (0x2120e78, 0x2120f44),
    (0x2152a3c, 0x2152a68),
    (0x218200c, 0x2182028),
    (0x21822f8, 0x21823c8),
    (0x21820b4, 0x2182124),
    (0x2181f3c, 0x2181f98),
    (0x219813c, 0x21981a4),
    (0x267816c, 0x267818c),
    (0x25bfa9c, 0x25bfae8),
    (0x25bfae8, 0x25bfaf0),
    (0x25bfa1c, 0x25bfa9a),
    (0x25bf992, 0x25bf998),
    (0x25c4eb4, 0x25c4eb8),
    (0x25c4bf0, 0x25c4bf4),
    (0x25c508c, 0x25c5090),
    (0x25c4d38, 0x25c4d3a),
    (0x25c4ef0, 0x25c4ef4),
    (0x25c4c2c, 0x25c4c30),
    (0x25c50c8, 0x25c50cc),
    (0x25c4d56, 0x25c4d58),
    (0x25bfaf0, 0x25bfb1c),
    (0x25bfb60, 0x25bfb68),
    (0x215ded8, 0x215def4),
    (0x215df0c, 0x215df20),
    (0x21975ec, 0x219764c),
    (0x216d768, 0x216d788),
    (0x2181fdc, 0x218200c),
    (0x216d768, 0x216d788),
    (0x2121118, 0x21211d0),
    (0x212ed1c, 0x212ed4c),
    (0x216d798, 0x216d7ec),
    (0x2164038, 0x21640c0),
    (0x2197f58, 0x2197f80),
    (0x216d74c, 0x216d768),
    (0x121b3a8, 0x121b474),
    (0x2120e78, 0x2120f44),
    (0x25c431c, 0x25c4330),
    (0x25c4330, 0x25c4348),
    (0x25c4348, 0x25c4350),
    (0x25cbf68, 0x25cbf98),
    (0x25c4d9c, 0x25c4da0),
    (0x25c4ad8, 0x25c4adc),
    (0x25c4f74, 0x25c4f78),
    (0x25c4cac, 0x25c4cae),
    (0x212ecec, 0x212ed1c),
    (0x2182124, 0x2182160),
    (0x2162370, 0x21623c0),
    (0x212cfb0, 0x212d104),
    (0x215df20, 0x215df38),
    (0x12357fc, 0x1235918),
    (0x121b3a8, 0x121b474),
    (0x15ff900, 0x15ff908),
    (0x15ffb7c, 0x15ffb8a),
    (0x1236590, 0x1236604),
    (0x1236a14, 0x1236a5c),
    (0x1236c60, 0x1236e20),
    (0x1236e20, 0x1237558),
    (0x127e784, 0x127e914),
    (0x216d74c, 0x216d788),
    (0x2120e78, 0x2120f44),
    (0x21529b8, 0x21529f8),
    (0x2152a3c, 0x2152a54),
    (0x2181fdc, 0x2182028),
    (0x212cfb0, 0x212d104),
    (0x2129958, 0x2129974),
    (0x21299b0, 0x21299e0),
    (0x2129b34, 0x2129bcc),
    (0x2152b94, 0x2152be4),
    (0x2181c50, 0x2181d50),
    (0x25bf974, 0x25bfa1c),
    (0x25bfaf0, 0x25bfb60),
    (0x25bfb60, 0x25bfb68),
    (0x25c4eb4, 0x25c4eb8),
    (0x25c4bf0, 0x25c4bf4),
    (0x25c508c, 0x25c5090),
    (0x25c4d38, 0x25c4d3a),
    (0x2181d98, 0x2181e54),
    (0x2180000, 0x2181f1c),
    (0x2129958, 0x2129e00),
    (0x21529b8, 0x21529f8),
    (0x2152a90, 0x2152aa4),
    (0x2152b1c, 0x2152b30),
    (0x2152b6c, 0x2152b94),
    (0x2120e78, 0x2120f44),
    (0x222443c, 0x2224474),
    (0x25bf974, 0x25bfa1c),
    (0x25bfaf0, 0x25bfb60),
    (0x25bfb60, 0x25bfb68),
    (0x25c4eb4, 0x25c4eb8),
    (0x25c4bf0, 0x25c4bf4),
    (0x25c508c, 0x25c5090),
    (0x25c4d38, 0x25c4d3a),
    (0x2172e64, 0x2172f24),
    (0x2172f24, 0x2172f44),
    (0x212a32c, 0x212a350),
    (0x2120e78, 0x2120f44),
    (0x2153f54, 0x2153f64),
    (0x2129e00, 0x212a1d4),
    (0x21813e4, 0x2181410),
    (0x21802ac, 0x21802bc),
    (0x216dae4, 0x216db00),
    (0x2121018, 0x2121118),
    (0x212eb64, 0x212ecec),
    (0x216dc4c, 0x216dca8),
    (0x216df64, 0x216e050),
    (0x2161cf8, 0x2161d14),
    (0x2161bb8, 0x2161cf8),
    (0x2161abc, 0x2161bb8),
    (0x2161d48, 0x2161d78),
    (0x2152c20, 0x2153d28),
    (0x1281848, 0x1281a40),
    (0x25c4ec8, 0x25c4ecc),
    (0x25c4c04, 0x25c4c08),
    (0x25c50a0, 0x25c50a4),
    (0x25c4d42, 0x25c4d44),
    (0x25c2a74, 0x25c2a76),
    (0x25c2eb4, 0x25c2ec0),
    (0x25c2ec0, 0x25c2ec8),
    (0x25c4eb8, 0x25c4ebc),
    (0x25c4bf4, 0x25c4bf8),
    (0x25c5090, 0x25c5094),
    (0x25c4d3a, 0x25c4d3c),
    (0x25c2c8c, 0x25c2e06),
    (0x25c2e18, 0x25c2eac),
    (0x25c2eac, 0x25c2eb4),
    (0x25c4d98, 0x25c4d9c),
    (0x25c4ad4, 0x25c4ad8),
    (0x25c4f70, 0x25c4f74),
    (0x25c4caa, 0x25c4cac),
    (0x25c4024, 0x25c404e),
    (0x25c4050, 0x25c4078),
    (0x25c4078, 0x25c4080),
    (0x25c51b8, 0x25c51c8),
    (0x25c5144, 0x25c514c),
    (0x25c5184, 0x25c5194),
    (0x25c51b0, 0x25c51b8),
    (0x2152c20, 0x2153d28),
    (0x2129e00, 0x212a1d4),
    (0x2172f44, 0x2173b48),
    (0x2170c78, 0x2170c98),
    (0x216254c, 0x2162564),
    (0x2162370, 0x21623c0),
    (0x25c4eb8, 0x25c4ebc),
    (0x25c4bf4, 0x25c4bf8),
    (0x25c4d3a, 0x25c4d3c),
    (0x25c2d1c, 0x25c2d2e),
    (0x25c2dac, 0x25c2dbe),
    (0x25c2df4, 0x25c2e06),
    (0x25c2e5c, 0x25c2e60),
    (0x25c2e6c, 0x25c2e70),
    (0x25c2e94, 0x25c2e98),
    (0x25c2e98, 0x25c2e9c),
    (0x25c2e9c, 0x25c2ea0),
    (0x2180000, 0x2181f1c),
    (0x2129958, 0x2129e00),
    (0x212fedc, 0x212ff20),
    (0x212a308, 0x212a32c),
    (0x2153f44, 0x2153f54),
    (0x21529b8, 0x21529f8),
    (0x2162370, 0x21623c0),
    (0x2120e78, 0x2120f44),
    (0x2129b0c, 0x2129b1c),
    (0x2129b4c, 0x2129b5c),
    (0x2129ce0, 0x2129cf0),
    (0x2129d08, 0x2129d18),
    (0x2129d68, 0x2129d78),
    (0x2129d90, 0x2129da0),
    (0x2129db8, 0x2129dc8),
    (0x212fee8, 0x212ff00),
    (0x2130210, 0x2130228),
    (0x2130264, 0x213027c),
    (0x2130348, 0x2130360),
    (0x2130368, 0x2130380),
    (0x21303e4, 0x21303fc),
    (0x21304c0, 0x21304d8),
    (0x2131ddc, 0x2131df4),
    (0x21328d8, 0x21328f0),
    (0x2132bbc, 0x2132bd4),
    (0x2132dbc, 0x2132dd4),
    (0x2133278, 0x2133290),
    (0x213335c, 0x2133374),
    (0x2133408, 0x2133420),
    (0x2157384, 0x215739c),
    (0x2157464, 0x215747c),
    (0x2158d40, 0x2158d58),
    (0x215df88, 0x215df98),
    (0x215dfc8, 0x215dfd8),
    (0x215e088, 0x215e098),
    (0x2169dfc, 0x2169e14),
    (0x2182228, 0x2182238),
    (0x21822d8, 0x21822e8),
    (0x21822e4, 0x21822f4),
    (0x2182308, 0x2182318),
    (0x2182b78, 0x2182b90),
    (0x25c4ec4, 0x25c4ec8),
    (0x25c4c00, 0x25c4c04),
    (0x25c509c, 0x25c50a0),
    (0x25c4d40, 0x25c4d42),
    (0x25bf860, 0x25bf862),
    (0x25bfb70, 0x25bfb74),
    (0x25bfb74, 0x25bfb7c),
    (0x25c4eb4, 0x25c4eb8),
    (0x25c4bf0, 0x25c4bf4),
    (0x25c508c, 0x25c5090),
    (0x25c4d38, 0x25c4d3a),
    (0x25bf97a, 0x25bf980),
    (0x25bf980, 0x25bf986),
    (0x25bfb04, 0x25bfb10),
    (0x25bfb60, 0x25bfb68),
    (0x2180518, 0x2180608),
    (0x2180248, 0x218028c),
    (0x2162370, 0x21623c0),
    (0x21cb6fc, 0x21cb7c8),
    (0x21ee518, 0x21ee538),
    (0x21f09d0, 0x21f0a40),
    (0x21e7374, 0x21e739c),
    (0x21ef1d4, 0x21ef1e8),
    (0x21cde88, 0x21cded8),
    (0x21cdfc8, 0x21cdfe8),
    (0x21ce258, 0x21ce270),
    (0x21f1f10, 0x21f1fe8),
    (0x21cf8b0, 0x21cf8fc),
    (0x21ce270, 0x21ce2c4),
    (0x21ce360, 0x21ce380),
    (0x21ce4f4, 0x21ce50c),
    (0x25eaad8, 0x25eaadc),
    (0x25eaaa8, 0x25eaaac),
    (0x25eaaf8, 0x25eaafc),
    (0x25eaac8, 0x25eaaca),
    (0x25ea3fc, 0x25ea406),
    (0x25eaa50, 0x25eaa58),
    (0x25eaaa0, 0x25eaaa8),
    (0x25eaadc, 0x25eaae0),
    (0x25eaaac, 0x25eaab0),
    (0x25eaafc, 0x25eab00),
    (0x25eaaca, 0x25eaacc),
    (0x25ea4d0, 0x25ea4da),
    (0x25ea8dc, 0x25ea8e4),
    (0x25ea91c, 0x25ea924),
    (0x1281848, 0x1281a40),
    (0x21cfcb8, 0x21cfcf0),
    (0x21ce50c, 0x21ce560),
    (0x21ce644, 0x21ce664),
    (0x21ce8ac, 0x21ce8c0),
    (0x21ce8c0, 0x21ce8c4),
    (0x21cff58, 0x21cff88),
    (0x21e8dec, 0x21e8e1c),
    (0x223d3d0, 0x223d400),
    (0x223d300, 0x223d348),
    (0x21d67ec, 0x21d6804),
    (0x25eaae0, 0x25eaae4),
    (0x25eaab0, 0x25eaab4),
    (0x25eab00, 0x25eab04),
    (0x25eaacc, 0x25eaace),
    (0x25ea55c, 0x25ea564),
    (0x25ea604, 0x25ea60c),
    (0x25ea65c, 0x25ea664),
    (0x2256424, 0x2256464),
    (0x2252188, 0x22521f4),
    (0x22522f8, 0x2252388),
    (0x2245fcc, 0x2245fec),
    (0x2245fec, 0x2246034),
    (0x2252db8, 0x2252dc0),
    (0x225343c, 0x2253454),
    (0x223dd0c, 0x223ddbc),
    (0x223dcac, 0x223dd0c),
    (0x2251534, 0x2251674),
    (0x2251760, 0x225179c),
    (0x22465dc, 0x224666c),
    (0x2246e14, 0x2246e28),
    (0x223e1bc, 0x223e1dc),
    (0x223d3c0, 0x223d3d0),
    (0x12aa500, 0x12aa544),
    (0x1263954, 0x1263974),
    (0x12638d0, 0x1263918),
    (0x225247c, 0x2252488),
    (0x225318c, 0x2253198),
    (0x22537f4, 0x2253800),
    (0x2252188, 0x2252278),
    (0x2253378, 0x22533a4),
    (0x22533ec, 0x2253458),
    (0x2252cd8, 0x2252dc4),
    (0x224576c, 0x22457c4),
    (0x22491fc, 0x224925c),
    (0x22493c4, 0x2249414),
    (0x2249414, 0x224944c),
    (0x2249178, 0x22491a4),
    (0x22491d0, 0x22491e4),
    (0x224a620, 0x224a654),
    (0x224a37c, 0x224a398),
    (0x223f530, 0x223f58c),
    (0x223f4c0, 0x223f518),
];

pub fn qualify(ram: &[u8]) -> Result<(), String> {
    for (ranges, length, expected) in [
        (RANGES.as_slice(), 692, SOURCE_SHA256),
        (
            COMPOSITION_RANGES.as_slice(),
            1872,
            COMPOSITION_SOURCE_SHA256,
        ),
        (FLOW_RANGES.as_slice(), 4196, FLOW_SOURCE_SHA256),
        (
            CONTINUATION_RANGES.as_slice(),
            57092,
            CONTINUATION_SOURCE_SHA256,
        ),
    ] {
        let mut bytes = Vec::with_capacity(length);
        for &(start, end) in ranges {
            bytes.extend_from_slice(ram.get(start..end).ok_or("Missing REC display source")?);
        }
        if bytes.len() != length || fujimod_firmware::sha256(&bytes) != expected {
            return Err("REC display source differs".into());
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct Trace {
    pub enabled: bool,
    pub records: Vec<Value>,
    pub dropped: usize,
}
impl Trace {
    pub fn value(&self) -> Value {
        json!({"enabled":self.enabled,"sourceSha256":SOURCE_SHA256,
            "compositionSourceSha256":COMPOSITION_SOURCE_SHA256,
            "flowSourceSha256":FLOW_SOURCE_SHA256,
            "continuationSourceSha256":CONTINUATION_SOURCE_SHA256,"limit":LIMIT,
            "records":self.records,"dropped":self.dropped,
            "meaning":"Passive pre-instruction CORE/CPSR samples at selected display/composition/flow/continuation/configuration/application/work-state/initialization/private-group2/1260-continuation/1287-software-gates/deferred-consumer/async-worker/raw1266-callback/queue25-shared-builder PCs; no deduplication or inferred MMIO commit/owner identity. Source-qualified group87/group2/group72/group73/group77/private-init0/private-init1/private-init2/raw140e/caller/header/code-target/mode2-selector/raw1287-header/raw1293-header filters. Optional qualified actual-core4B wrapper-frame and2B app-message host-context reads may affect softTLB; capacity/drops bound coverage."})
    }
}

pub fn observe(board: &Session, trace: &mut Trace, core: usize, pc: u64) {
    if !trace.enabled || !POINTS.contains(&pc) {
        return;
    }
    let registers = arm::CORE
        .iter()
        .map(|&r| board.read_register(core, r))
        .collect::<Result<Vec<_>, _>>();
    if pc == 0x5a9f8 && registers.as_ref().is_ok_and(|r| r[0] != 0xfffa9008) {
        return;
    }
    if let Ok(r) = &registers {
        let selected = match pc {
            0x121e13c => r[3] == 0x140e,
            0x121de40 => r[0] == 0x140e,
            0x2120edc | 0x2120ef8 | 0x2120efc => matches!(r[4], 87 | 2 | 72 | 73 | 77),
            0x2120f3c => matches!(r[4], 0xae | 4 | 144 | 146 | 154) && r[2] == 0x31ecc4c,
            0x216d768 => matches!(r[14], 0x2182100 | 0x2182140),
            0x2170c78 => matches!(r[14], 0x2153a54 | 0x2153a94),
            0x21f1f84 | 0x21f1f98 | 0x21f1f9c => matches!(r[7], 0..=2),
            0x2256460 | 0x22521f0 => r[5] == 0x1260,
            0x2245fcc => r[14] == 0x2252380,
            0x2246008 => r[3] == 0x21d67ec,
            0x2251560 => r[0] == 2,
            0x2252274 => r[5] == 0x1287,
            _ => true,
        };
        if !selected {
            return;
        }
    }
    let header_cpsr = (pc == 0x2249258).then(|| board.read_register(core, arm::CPSR));
    if let (Ok(r), Some(Ok(status))) = (&registers, &header_cpsr)
        && *status & (0x20 | 0x1000000) == 0
        && r[15] == pc
        && r[2] != 0x1293
    {
        return;
    }
    if trace.records.len() == LIMIT {
        trace.dropped = trace.dropped.saturating_add(1);
        return;
    }
    let phase = match pc {
        0x122e66c => "refresh-entry-before-push",
        0x12310cc => "lcdc1-groups-before-call",
        0x5a9f8 => "write32-before-store",
        0x121a3b8 => "picture-public-entry-before-validation",
        0x122c814 => "picture-owner-entry-before-push",
        0x122c838 => "picture-owner-return-before-unwind",
        0x1246d4c => "composition-parent-entry-before-push",
        0x124688c => "composition-helper-entry-before-push",
        0x1246974 => "composition-helper-result-before-unwind",
        0x1248930 => "composition-iterator-entry-before-push",
        0x1248974 => "composition-iterator-head-before-call",
        0x124871c => "composition-walker-prepare-return",
        0x1248730 => "composition-walker-first-before-empty-branch",
        0x1246a1c => "composition-callback-entry-before-push",
        0x1246a40 => "composition-callback-gate4-return",
        0x1246a5c => "composition-callback-selector-before-compare",
        0x1246a78 => "composition-method-other-before-blx",
        0x1246ac8 => "composition-method-zero-before-blx",
        0x1246af8 => "composition-method-four-before-blx",
        0x1246a88 => "composition-accumulator-other-before-or",
        0x1246ad8 => "composition-accumulator-zero-before-or",
        0x1246b08 => "composition-accumulator-four-before-or",
        0x1246958 => "composition-helper-float-promotion-condition",
        0x12466d0 => "composition-dispatch-message-before-compare",
        0x12466e0 => "composition-dispatch-state-before-compare",
        0x12466f4 => "composition-dispatch-before-indirect-call",
        0x1247120 => "composition-transaction-entry-before-push",
        0x124bc88 => "list-acquire-count-before-compare",
        0x1248844 => "list-init-result-before-return",
        0x1248888 => "list-insert-append-return",
        0x12488bc => "list-remove-entry-before-push",
        0x1248908 => "list-remove-success-before-counter-load",
        0x124bdb4 => "list-release-count-stored-before-compare",
        0x1248860 => "list-insert-entry-before-index-check",
        0x124bd90 => "list-release-entry-before-push",
        0x2177a18 => "video-config-a-buffer-gate-before-compare",
        0x2177c88 => "video-config-b-entry-before-push",
        0x2177fd8 => "video-config-b-buffer-gate-before-compare",
        0x21980f0 => "video-forwarder-entry-before-push",
        0x121a36c => "video-public-entry-before-validation",
        0x122c868 => "video-owner-entry-before-push",
        0x122c888 => "video-selector0-before-f9-store",
        0x122c89c => "video-selector1-before-f8-store",
        0x122c8a4 => "video-owner-result-before-unwind",
        0x219813c => "video-continuation-entry-before-push",
        0x219817c => "video-continuation-after-zero-handle-operation",
        0x2198190 => "video-continuation-result-before-pop",
        0x1218f04 => "video-admission-enabled-before-compare",
        0x121e0f4 => "video-140e-consumer-before-call",
        0x1219074 => "video-config-consumer-entry-before-push",
        0x12190b0 => "video-config-initial-fields-before-compare",
        0x1219124 => "video-config-byte0-after-load-before-compare",
        0x1219244 => "video-config-change-fields-after-load",
        0x12191dc => "video-config-fast-callback-before-blx",
        0x12198cc => "video-config-late-callback-before-blx",
        0x121e13c => "video-140e-task-message-before-compare",
        0x12191e0 => "video-config-fast-callback-return",
        0x12198d0 => "video-config-late-callback-return",
        0x121de40 => "video-140e-submit-before-call",
        0x219818c => "video-continuation-submit-before-call",
        0x1218f38 => "video-admission-enqueue-return-before-success-overwrite",
        0x21821e0 => "video-callback-request-before-call",
        0x2181fc0 => "video-callback-entry-before-push",
        0x212d010 => "video-callback-111d-match-before-branch",
        0x2120edc => "video-callback-state-cell-before-action-load",
        0x2120ef8 => "video-callback-state-action-before-blx",
        0x21822f8 => "video-callback-post111d-entry-before-push",
        0x2182348 => "video-callback-predicate-return-before-compare",
        0x218236c => "video-callback-config-flag-before-call",
        0x212cff8 => "video-callback-1114-match-before-branch",
        0x2120efc => "video-callback-action-return-before-save",
        0x2120f3c => "video-callback-state-before-halfword-store",
        0x216d768 => "video-callback-config-callee-entry-before-push",
        0x212cfc4 => "video-callback-router-mode-before-compare",
        0x216d7d0 => "config-predicate-before-call",
        0x216d7d4 => "config-predicate-return-before-callback-constant",
        0x2197f58 => "config-forwarder-entry-before-push",
        0x121b3a8 => "config-public-entry-before-validation",
        0x121b460 => "config-async-submit-before-call",
        0x121b464 => "config-async-submit-return-before-branch",
        0x2197f7c => "config-public-return-before-forwarder-pop",
        0x216d74c => "config-completion-callback-entry-before-push",
        0x2182140 => "config-command7-return-before-compare",
        0x12358fc => "config-queue-return-before-compare",
        0x1235908 => "config-public-result-before-epilogue",
        0x12373d0 => "config-pending-byte-after-load",
        0x12373e0 => "config-opcode-before-compare",
        0x1236f44 => "config-copied-byte0-after-load",
        0x1236f50 => "config-copied-byte1-after-load",
        0x12370d4 => "config-work-helper-return-before-test",
        0x12370e8 => "config-immediate-callback-before-blx",
        0x12370ec => "config-immediate-callback-after-return",
        0x123720c => "config-pending-event-after-load",
        0x1237254 => "config-pending-callback-before-blx",
        0x1237258 => "config-pending-callback-after-return",
        0x1236c6c => "config-work-class-after-load-before-test",
        0x2181c50 => "app-predicate-entry-before-push",
        0x2181c88 => "app-predicate-query-byte-before-compare",
        0x2181ca8 => "app-predicate-first-mask-after-bics",
        0x2181cc4 => "app-predicate-second-flags-before-and",
        0x2181ccc => "app-predicate-second-mask-before-moveq",
        0x2152ba0 => "app-action26-predicate-return-before-save",
        0x2152bbc => "app-action26-state2-before-conditional-store",
        0x2152bd8 => "app-action26-state0-before-store",
        0x21807c4 => "app-flags-setter-entry-before-push",
        0x21807e4 => "app-flags-setter-before-store",
        0x2181ef0 => "app-flags-or1-before-store",
        0x218077c => "app-flags-or1800-query13-before-store",
        0x2181530 => "app-flags-or1800-query9-before-conditional-store",
        0x21818d0 => "app-flags-or1800-saved-query13-before-conditional-store",
        0x2180ae4 => "app-flags-or2000-query4-before-store",
        0x2180b64 => "app-flags-or2000-query2-before-store",
        0x2181254 => "app-flags-init-or2000-before-store",
        0x2180fe8 => "app-flags-init-zero-before-store",
        0x2180728 => "app-flags-query13-first-mask-before-returneq",
        0x218075c => "app-flags-query13-return-before-compare",
        0x21814ec => "app-flags-query9-mask21-before-compare",
        0x218151c => "app-flags-query9-return-before-compare",
        0x21818bc => "app-flags-query13-saved-result-before-compare",
        0x2180abc => "app-flags-query4-return-before-compare",
        0x2180b14 => "app-flags-query2-local-byte-before-compare",
        0x2180b3c => "app-flags-query2-return-before-compare",
        0x2180d0c => "app-flags-init-entry-before-push",
        0x2181220 => "app-flags-init-byte-before-compare2",
        0x2181ed8 => "app-flags-or1-byte-before-compare1",
        0x21539e0 => "work-state4-mask-after-bics-before-branch",
        0x21539fc => "work-state4-param74-before-compare85",
        0x2153a08 => "work-state4-work-bit8-after-test",
        0x2153a28 => "work-state4-param7a-before-compare1",
        0x2153a38 => "work-state4-param76-before-compare2",
        0x2170c78 => "work-state4-selector-delegate-entry",
        0x2153a54 => "work-state4-selector-result-before-compare1",
        0x2153ab4 => "work-state4-bit6-before-store",
        0x2153a10 => "work-state4-leaf-result-before-unwind",
        0x2173ac8 => "work-message1113-construction-entry",
        0x2173ae4 => "work-message1113-submit-before-call",
        0x2173ae8 => "work-message1113-submit-wrapper-return",
        0x212a190 => "work-query13-bit17-before-store",
        0x2153058 => "work-event8-state0-composite-or-before-store",
        0x2153180 => "work-event16-state6-bit6-before-store",
        0x21539b0 => "work-event20-state3-bit6-before-store",
        0x2153b94 => "work-event20-state5-bit6-before-store",
        0x2172f78 => "work-workword-initialization-zero-before-store",
        0x212a008 => "work-query9-bit8-before-store",
        0x2172e64 => "work-query-entry-before-push",
        0x21cb6fc => "init-delegate-entry-before-push",
        0x21cb728 => "init-first-helper-result-before-compare0",
        0x21cb770 => "init-code-address-payload-before-store",
        0x21f0a24 => "init-work-copy-before-call",
        0x21cb7b8 => "init-private-dispatch-result-before-overwrite",
        0x21cb7c0 => "init-last-helper-result-before-unwind",
        0x21805ec => "init-delegate-caller-return",
        0x2180248 => "init-code0-entry-before-push",
        0x2180278 => "init-code1-condition-before-returnne",
        0x2180284 => "init-code1-before-delegate-call",
        0x21f09d0 => "init-work-copy-entry-before-push",
        0x218028c => "init-class-setter-entry-before-push",
        0x21cded4 => "init-private-header-match-before-branch",
        0x21f1f84 => "init-private-cell-before-action-index-load",
        0x21f1f98 => "init-private-precall-target",
        0x21f1f9c => "init-private-action-return-before-save",
        0x21cfcc4 => "init-private2-predicate-return-before-cmp",
        0x21ce55c => "init-private2-header-match-before-branch",
        0x21ce65c => "init-private2-dispatch-return-before-save",
        0x21e8e10 => "init-private2-delegate-submit-before-bl",
        0x223d338 => "init-private2-queue-raw-return-before-cmp",
        0x21cff6c => "init-private2-action-delegate-return-before-forced-zero",
        0x21d67ec => "init-private2-referenced-code-entry-before-push",
        0x2256460 => "init-1260-outer-header-match-before-branch",
        0x22521f0 => "init-1260-inner-header-match-before-branch",
        0x2245fcc => "init-1260-code-setter-entry-before-save-fp",
        0x2246008 => "init-1260-code-target-before-blx",
        0x223dd68 => "init-software-class-byte-pre-store",
        0x223dd78 => "init-software-setting-bit-return",
        0x223dda0 => "init-software-validator-return",
        0x223dd8c => "init-software-flag-zero-pre-store",
        0x223ddb4 => "init-software-flag-one-pre-store",
        0x2251560 => "init-1287-mode2-selector",
        0x2251624 => "init-1287-first-flag-return",
        0x2251630 => "init-1287-second-flag-return",
        0x2251658 => "init-1287-fresh-state-before-compare",
        0x2246660 => "init-1287-dedicated-builder-before-call",
        0x2252274 => "init-1287-inner-header-match-before-branch",
        0x225339c => "init-deferred-predicate-return-before-cmp",
        0x225340c => "init-deferred-current-state-before-jump-table",
        0x2251534 => "init-mode-selector-entry-before-prologue",
        0x224577c => "init-deferred-flags-word-after-load-before-mask",
        0x22457a4 => "init-deferred-blocker-word-after-load-before-compare",
        0x2249258 => "async-worker-raw1293-header-after-guest-load",
        0x22493dc => "async-worker-stage0-before-original-bl",
        0x22493e0 => "async-worker-stage0-raw-continuation-before-save",
        0x2249434 => "async-worker-optional-class7-before-original-bl",
        0x2249438 => "async-worker-optional-class7-raw-continuation",
        0x2249448 => "async-worker-step1-after-status-zero-move",
        0x22491d8 => "async-worker-mode0-callback-before-original-blx",
        0x223f530 => "raw1266-callback-original-entry-before-prologue",
        0x223f500 => "queue25-shared-builder-before-original-send-bl",
        0x223f504 => "queue25-shared-builder-raw-send-continuation",
        _ => unreachable!(),
    };
    let cpsr = header_cpsr.unwrap_or_else(|| board.read_register(core, arm::CPSR));
    let mut record = json!({"core":core,"pc":pc,"tick":board.now(),
        "instructions":board.instructions().iter().sum::<u64>(),"phase":phase});
    let mut unavailable = serde_json::Map::new();
    match &registers {
        Ok(r) => {
            record["registers"] = json!(r);
            record["sp"] = json!(r[13]);
            record["fp"] = json!(r[11]);
        }
        Err(error) => {
            unavailable.insert("registers".into(), json!(error));
        }
    }
    match &cpsr {
        Ok(value) => record["preCpsr"] = json!(value),
        Err(error) => {
            unavailable.insert("preCpsr".into(), json!(error));
        }
    }
    if !unavailable.is_empty() {
        record["unavailable"] = Value::Object(unavailable);
    }
    if pc == 0x122c814 {
        let saved = (|| -> Result<Value, String> {
            let r = registers.as_ref().map_err(Clone::clone)?;
            let status = *cpsr.as_ref().map_err(Clone::clone)?;
            if status & 0x20 != 0 || r[14] != 0x121a3e8 {
                return Err("Display wrapper caller/ARM state unqualified".into());
            }
            let fp = u32::try_from(r[11]).map_err(|_| "Display FP exceeds ARM32")?;
            let sp = u32::try_from(r[13]).map_err(|_| "Display SP exceeds ARM32")?;
            if (fp | sp) & 3 != 0
                || sp.checked_add(12) != Some(fp)
                || fp
                    .checked_add(4)
                    .is_none_or(|end| u64::from(end) > board.ram_bytes() as u64)
            {
                return Err("Display wrapper frame outside declared aligned RAM".into());
            }
            Ok(json!({"address":fp,"bytes":board.read_virtual(core,fp,4)?}))
        })();
        record["savedWrapperCaller"] = saved.unwrap_or_else(|error| json!({"unavailable":error}));
    }
    if pc == 0x212cfc4 {
        let context = (|| -> Result<Value, String> {
            let r = registers.as_ref().map_err(Clone::clone)?;
            let status = *cpsr.as_ref().map_err(Clone::clone)?;
            if status & (0x20 | 0x1000000) != 0
                || !matches!(r[14], 0x2182008 | 0x2182024)
                || r[3] > 255
            {
                return Err("App message caller/ARM state unqualified".into());
            }
            let fp = u32::try_from(r[11]).map_err(|_| "App FP exceeds ARM32")?;
            let sp = u32::try_from(r[13]).map_err(|_| "App SP exceeds ARM32")?;
            if (fp | sp) & 3 != 0
                || sp.checked_add(4) != Some(fp)
                || fp
                    .checked_add(4)
                    .is_none_or(|end| u64::from(end) > board.ram_bytes() as u64)
                || 0x2d0bf4a > board.ram_bytes()
            {
                return Err("App message context outside declared aligned RAM".into());
            }
            Ok(
                json!({"address":0x2d0bf48u32,"bytes":board.read_virtual(core,0x2d0bf48,2)?,
                "meaning":"Host current-message context peek; not guest LDRH or unique producer/delivery receipt."}),
            )
        })();
        record["appMessageHostContext"] =
            context.unwrap_or_else(|error| json!({"unavailable":error}));
    }
    trace.records.push(record);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn all_registers(board: &Session) -> Vec<Vec<u64>> {
        (0..4)
            .map(|core| {
                arm::CORE
                    .iter()
                    .copied()
                    .chain([arm::CPSR])
                    .map(|r| board.read_register(core, r).unwrap())
                    .collect()
            })
            .collect()
    }
    #[test]
    fn display_guard_accepts_original_fixture_and_rejects_each_changed_range() {
        assert!(qualify(&[]).is_err());
        let fixtures = [
            (
                RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-display.bin").as_slice(),
            ),
            (
                COMPOSITION_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-composition.bin").as_slice(),
            ),
            (
                FLOW_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-flow.bin").as_slice(),
            ),
            (
                CONTINUATION_RANGES.as_slice(),
                include_bytes!("../../../profiles/fixtures/xt4-212-rec-continuation.bin")
                    .as_slice(),
            ),
        ];
        assert_eq!(
            (
                fixtures[0].1.len(),
                fixtures[1].1.len(),
                fixtures[2].1.len(),
                fixtures[3].1.len()
            ),
            (692, 1872, 4196, 57092)
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..7384]),
            "3aabab3c6bea2ac67d2a1f5931ae0f336f01cc011a5b0ae4af376dd289f369aa"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..12464]),
            "b3af058477704cc27f5ef9e9cb4b3345fd96f6bdfaf41824ad8ce0143d8e99ff"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..24088]),
            "104c29c8a8d04b6137548f91261ea4fb54393654e9a4ffa992f8eaab4c108908"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..41496]),
            "c97180877947fe5330310b83acf9229c1155f031ee13cb924d3d439c9cf68192"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..53822]),
            "02f5994488ee1a984dc0c06eaa85dbc04684308df7efcc04438b30d468496d88"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..54748]),
            "bbd48efe868c5e3e3d0b9ee5af57382c58eda02d38fd07161e5f5c1377216c97"
        );
        assert_eq!(
            fujimod_firmware::sha256(&fixtures[3].1[..56536]),
            "e2b72785ace73fe34e0efd436738b7cc9e12c55fad7463463ad14356855e0c72"
        );
        let max_end = fixtures
            .iter()
            .flat_map(|(ranges, _)| ranges.iter().map(|&(_, end)| end))
            .max()
            .unwrap();
        let mut ram = vec![0; max_end];
        for (ranges, bytes) in fixtures {
            let mut cursor = 0;
            for &(start, end) in ranges {
                ram[start..end].copy_from_slice(&bytes[cursor..cursor + end - start]);
                cursor += end - start;
            }
            assert_eq!(cursor, bytes.len());
        }
        qualify(&ram).unwrap();
        for (ranges, _) in fixtures {
            for &(start, _) in ranges {
                ram[start] ^= 1;
                assert!(qualify(&ram).is_err());
                ram[start] ^= 1;
            }
        }
    }
    #[test]
    fn continuation_filters_are_stage_specific_and_keep_unavailable_rows() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace {
            enabled: true,
            ..Default::default()
        };
        for (pc, register, selected, rejected) in [
            (0x121e13c, 3, 0x140e, 0x140f),
            (0x121de40, 0, 0x140e, 0x140f),
            (0x2120edc, 4, 87, 88),
            (0x2120edc, 4, 2, 3),
            (0x2120edc, 4, 72, 74),
            (0x2120edc, 4, 73, 74),
            (0x2120edc, 4, 77, 78),
            (0x2120ef8, 4, 87, 88),
            (0x2120ef8, 4, 2, 3),
            (0x2120ef8, 4, 72, 74),
            (0x2120ef8, 4, 73, 74),
            (0x2120ef8, 4, 77, 78),
            (0x2120efc, 4, 87, 88),
            (0x2120efc, 4, 2, 3),
            (0x2120efc, 4, 72, 74),
            (0x2120efc, 4, 73, 74),
            (0x2120efc, 4, 77, 78),
            (0x2120f3c, 4, 0xae, 87),
            (0x2120f3c, 4, 4, 5),
            (0x2120f3c, 4, 144, 145),
            (0x2120f3c, 4, 146, 147),
            (0x2120f3c, 4, 154, 155),
            (0x216d768, 14, 0x2182100, 0x2182104),
            (0x216d768, 14, 0x2182140, 0x2182144),
            (0x2170c78, 14, 0x2153a54, 0x2153a58),
            (0x2170c78, 14, 0x2153a94, 0x2153a98),
            (0x21f1f84, 7, 0, 3),
            (0x21f1f84, 7, 1, 3),
            (0x21f1f98, 7, 0, 3),
            (0x21f1f98, 7, 1, 3),
            (0x21f1f9c, 7, 0, 3),
            (0x21f1f9c, 7, 1, 3),
            (0x21f1f84, 7, 2, 3),
            (0x21f1f98, 7, 2, 3),
            (0x21f1f9c, 7, 2, 3),
            (0x2256460, 5, 0x1260, 0x1261),
            (0x22521f0, 5, 0x1260, 0x1261),
            (0x2245fcc, 14, 0x2252380, 0x2252384),
            (0x2246008, 3, 0x21d67ec, 0x21d67f0),
            (0x2251560, 0, 2, 3),
            (0x2251560, 0, 2, 0x102),
            (0x2252274, 5, 0x1287, 0x1286),
            (0x2252274, 5, 0x1287, 0x11287),
        ] {
            board.write_register(2, arm::CORE[2], 0x31ecc4c).unwrap();
            for (value, retained) in [(selected, true), (rejected, false)] {
                board.write_register(2, arm::CORE[register], value).unwrap();
                let before = serde_json::to_value(board.state(true)).unwrap();
                let registers = all_registers(&board);
                let count = trace.records.len();
                observe(&board, &mut trace, 2, pc);
                assert_eq!(trace.records.len(), count + usize::from(retained));
                assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
                assert_eq!(registers, all_registers(&board));
            }
        }
        board.write_register(2, arm::CORE[2], 0x31ecc50).unwrap();
        let count = trace.records.len();
        for scaled_group in [0xae, 4, 144, 146, 154] {
            board.write_register(2, arm::CORE[4], scaled_group).unwrap();
            observe(&board, &mut trace, 2, 0x2120f3c);
            assert_eq!(trace.records.len(), count);
        }
        observe(&board, &mut trace, 99, 0x2120edc);
        assert!(trace.records.last().unwrap()["unavailable"]["registers"].is_string());
        observe(&board, &mut trace, 99, 0x21f1f84);
        assert_eq!(trace.records.last().unwrap()["pc"], 0x21f1f84);
        assert!(trace.records.last().unwrap()["unavailable"]["registers"].is_string());
        for pc in [0x2251560, 0x2252274] {
            let before = trace.records.len();
            observe(&board, &mut trace, 99, pc);
            assert_eq!(trace.records.len(), before + 1);
            assert_eq!(trace.records.last().unwrap()["pc"], pc);
            assert!(trace.records.last().unwrap()["unavailable"]["registers"].is_string());
        }
        for pc in [
            0x223dd68, 0x223dd78, 0x223dda0, 0x223dd8c, 0x223ddb4, 0x2251624, 0x2251630, 0x2251658,
            0x2246660, 0x225339c, 0x225340c,
        ] {
            for raw in [0, 1, u32::MAX as u64] {
                board.write_register(2, arm::CORE[0], raw).unwrap();
                board.write_register(2, arm::CORE[3], 9).unwrap();
                board.write_register(2, arm::CORE[14], 0xdeadbeef).unwrap();
                let before = serde_json::to_value(board.state(true)).unwrap();
                let registers = all_registers(&board);
                let count = trace.records.len();
                observe(&board, &mut trace, 2, pc);
                assert_eq!(trace.records.len(), count + 1);
                assert_eq!(trace.records.last().unwrap()["registers"][0], raw);
                assert_eq!(
                    trace.records.last().unwrap()["registers"][14],
                    0xdeadbeefu64
                );
                assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
                assert_eq!(registers, all_registers(&board));
            }
        }
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn selector_and_loaded_predicate_operands_remain_raw_without_frame_or_value_filters() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace {
            enabled: true,
            ..Trace::default()
        };
        for pc in [0x2251534, 0x224577c, 0x22457a4] {
            for raw in [0, 2, 6, u32::MAX as u64] {
                for register in [0, 3] {
                    board.write_register(2, arm::CORE[register], raw).unwrap();
                }
                board.write_register(2, arm::CORE[11], 9).unwrap();
                board.write_register(2, arm::CORE[13], 1).unwrap();
                board.write_register(2, arm::CORE[14], 0xdeadbeef).unwrap();
                let before = serde_json::to_value(board.state(true)).unwrap();
                let registers = all_registers(&board);
                let count = trace.records.len();
                observe(&board, &mut trace, 2, pc);
                assert_eq!(trace.records.len(), count + 1);
                let row = trace.records.last().unwrap();
                assert_eq!(row["registers"][0], raw);
                assert_eq!(row["registers"][3], raw);
                assert_eq!(row["registers"][14], 0xdeadbeefu64);
                assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
                assert_eq!(registers, all_registers(&board));
            }
            let count = trace.records.len();
            observe(&board, &mut trace, 99, pc);
            assert_eq!(trace.records.len(), count + 1);
            assert!(trace.records.last().unwrap()["unavailable"]["registers"].is_string());
        }
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn async_worker_points_keep_raw_status_bad_frames_and_unavailable_without_board_changes() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace {
            enabled: true,
            ..Default::default()
        };
        let sentinel = [0xa5; 64];
        board.write(0x1f00000, &sentinel).unwrap();
        for &pc in &POINTS[188..] {
            for raw in [0, 0x100, u32::MAX as u64] {
                for register in [0, 3, 4] {
                    board.write_register(2, arm::CORE[register], raw).unwrap();
                }
                board
                    .write_register(2, arm::CORE[2], if pc == 0x2249258 { 0x1293 } else { raw })
                    .unwrap();
                board.write_register(2, arm::CORE[11], 9).unwrap();
                board.write_register(2, arm::CORE[13], 1).unwrap();
                board.write_register(2, arm::CORE[14], 0xdeadbeef).unwrap();
                board.write_register(2, arm::PC, pc).unwrap();
                board.write_register(2, arm::CPSR, 0x53).unwrap();
                let state = serde_json::to_value(board.state(true)).unwrap();
                let registers = all_registers(&board);
                let count = trace.records.len();
                observe(&board, &mut trace, 2, pc);
                assert_eq!(trace.records.len(), count + 1);
                let row = trace.records.last().unwrap();
                for register in [0, 3, 4] {
                    assert_eq!(row["registers"][register], raw);
                }
                assert_eq!(row["registers"][14], 0xdeadbeefu64);
                assert_eq!(row["sp"], 1);
                assert_eq!(row["fp"], 9);
                assert_eq!(row.as_object().unwrap().len(), 9);
                assert!(row.get("savedWrapperCaller").is_none());
                assert!(row.get("appMessageHostContext").is_none());
                assert_eq!(state, serde_json::to_value(board.state(true)).unwrap());
                assert_eq!(registers, all_registers(&board));
                let mut ram = [0; 64];
                board.read(0x1f00000, &mut ram).unwrap();
                assert_eq!(ram, sentinel);
            }
            let count = trace.records.len();
            observe(&board, &mut trace, 99, pc);
            assert_eq!(trace.records.len(), count + 1);
            let row = trace.records.last().unwrap();
            assert!(row["unavailable"]["registers"].is_string());
            assert!(row["unavailable"]["preCpsr"].is_string());
            assert!(row.get("savedWrapperCaller").is_none());
            assert!(row.get("appMessageHostContext").is_none());
        }
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn async_header_filter_excludes_only_matching_a32_pc_and_retains_unknown_phase() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace {
            enabled: true,
            ..Default::default()
        };
        for (status, raw_pc, header, retained) in [
            (0x53, 0x2249258, 0x1293, true),
            (0x53, 0x2249258, 0x1266, false),
            (0x53, 0x2249258, 0, false),
            (0x53, 0x2249258, u32::MAX as u64, false),
            (0x73, 0x2249258, 0x1266, true),
            (0x1000053, 0x2249258, 0x1266, true),
            (0x53, 0x224925c, 0x1266, true),
        ] {
            board.write_register(2, arm::PC, raw_pc).unwrap();
            board.write_register(2, arm::CPSR, status).unwrap();
            board.write_register(2, arm::CORE[2], header).unwrap();
            let state = serde_json::to_value(board.state(true)).unwrap();
            let registers = all_registers(&board);
            let count = trace.records.len();
            observe(&board, &mut trace, 2, 0x2249258);
            assert_eq!(trace.records.len(), count + usize::from(retained));
            if retained {
                let row = trace.records.last().unwrap();
                assert_eq!(row["registers"][2], header);
                assert_eq!(row["registers"][15], raw_pc);
                assert_eq!(row["preCpsr"], status);
            }
            assert_eq!(state, serde_json::to_value(board.state(true)).unwrap());
            assert_eq!(registers, all_registers(&board));
        }
        let count = trace.records.len();
        observe(&board, &mut trace, 99, 0x2249258);
        assert_eq!(trace.records.len(), count + 1);
        assert!(trace.records.last().unwrap()["unavailable"]["registers"].is_string());
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn app_message_context_is_two_raw_bytes_with_strict_source_frame_and_bounds() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":67108864,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        board.write(0x2d0bf48, &[0x1d, 0x11]).unwrap();
        for (lr, fp, sp, mode, status, ok) in [
            (0x2182008, 0x1f00004, 0x1f00000, 3, 0x53, true),
            (0x2182024, 0x1f00004, 0x1f00000, 0, 0x253, true),
            (0, 0x1f00004, 0x1f00000, 3, 0x53, false),
            (0x2182008, 0x1f00008, 0x1f00000, 3, 0x53, false),
            (0x2182008, 0x1f00005, 0x1f00001, 3, 0x53, false),
            (0x2182008, 0x1f00004, 0x1f00000, 256, 0x53, false),
            (0x2182008, 0x1f00004, 0x1f00000, 3, 0x73, false),
            (0x2182008, 0x1f00004, 0x1f00000, 3, 0x1000053, false),
        ] {
            for (r, value) in [(14, lr), (11, fp), (13, sp), (3, mode)] {
                board.write_register(2, arm::CORE[r], value).unwrap();
            }
            board.write_register(2, arm::CPSR, status).unwrap();
            let before = serde_json::to_value(board.state(true)).unwrap();
            let registers = all_registers(&board);
            let mut trace = Trace {
                enabled: true,
                ..Default::default()
            };
            observe(&board, &mut trace, 2, 0x212cfc4);
            let context = &trace.records[0]["appMessageHostContext"];
            if ok {
                assert_eq!(context["bytes"], json!([0x1d, 0x11]));
                assert_eq!(context["address"], 0x2d0bf48);
            } else {
                assert!(context["unavailable"].is_string());
            }
            assert_eq!(trace.records[0]["core"], 2);
            assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
            assert_eq!(registers, all_registers(&board));
        }
        let small = Session::new(
            serde_json::from_value(
                json!({"ram_size":33554432,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        for (r, value) in [(14, 0x2182008), (11, 0x1f00004), (13, 0x1f00000), (3, 3)] {
            small.write_register(2, arm::CORE[r], value).unwrap();
        }
        let mut trace = Trace {
            enabled: true,
            ..Default::default()
        };
        observe(&small, &mut trace, 2, 0x212cfc4);
        assert!(trace.records[0]["appMessageHostContext"]["unavailable"].is_string());
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }

    #[test]
    fn display_filter_capacity_and_unavailable_are_independent_of_guest_state() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":33554432,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        let mut trace = Trace::default();
        observe(&board, &mut trace, 0, 0x122e66c);
        assert!(trace.records.is_empty());
        trace.enabled = true;
        observe(&board, &mut trace, 0, 0x5a9f8);
        assert!(trace.records.is_empty());
        board.write_register(3, arm::CORE[0], 0xfffa9008).unwrap();
        let seeded = serde_json::to_value(board.state(true)).unwrap();
        let registers = all_registers(&board);
        observe(&board, &mut trace, 3, 0x5a9f8);
        assert_eq!(trace.records[0]["core"], 3);
        assert_eq!(trace.records[0]["registers"][0], 0xfffa9008u32);
        observe(&board, &mut trace, 99, 0x122e66c);
        assert!(trace.records[1]["unavailable"]["registers"].is_string());
        for _ in 0..LIMIT {
            observe(&board, &mut trace, 0, 0x122e66c);
        }
        assert_eq!((trace.records.len(), trace.dropped), (LIMIT, 2));
        assert_eq!(seeded, serde_json::to_value(board.state(true)).unwrap());
        assert_eq!(registers, all_registers(&board));
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }
    #[test]
    fn display_wrapper_read_is_four_bytes_on_actual_core_and_strictly_qualified() {
        let board = Session::new(
            serde_json::from_value(
                json!({"ram_size":33554432,"cores":4,"discovery":false,"cbar":4294901760u32}),
            )
            .unwrap(),
            None,
        )
        .unwrap();
        board.write(0x1f0000c, &[1, 2, 3, 4]).unwrap();
        for (core, fp, sp, lr, thumb, ok) in [
            (2, 0x1f0000c, 0x1f00000, 0x121a3e8, false, true),
            (2, 0x1f00010, 0x1f00000, 0x121a3e8, false, false),
            (2, 0x1f0000d, 0x1f00001, 0x121a3e8, false, false),
            (2, 0x1f0000c, 0x1f00000, 0, false, false),
            (2, 0x1f0000c, 0x1f00000, 0x121a3e8, true, false),
            (2, 0xfffa900c, 0xfffa9000, 0x121a3e8, false, false),
        ] {
            board.write_register(core, arm::CORE[11], fp).unwrap();
            board.write_register(core, arm::CORE[13], sp).unwrap();
            board.write_register(core, arm::CORE[14], lr).unwrap();
            let cpsr = (board.read_register(core, arm::CPSR).unwrap() & !0x20)
                | if thumb { 0x20 } else { 0 };
            board.write_register(core, arm::CPSR, cpsr).unwrap();
            let before = serde_json::to_value(board.state(true)).unwrap();
            let regs = all_registers(&board);
            let mut trace = Trace {
                enabled: true,
                ..Default::default()
            };
            observe(&board, &mut trace, core, 0x122c814);
            let read = &trace.records[0]["savedWrapperCaller"];
            if ok {
                assert_eq!(read["bytes"], json!([1, 2, 3, 4]));
                assert_eq!(read["address"], fp);
            } else {
                assert!(read["unavailable"].is_string());
            }
            assert_eq!(trace.records[0]["preCpsr"], cpsr);
            assert_eq!(before, serde_json::to_value(board.state(true)).unwrap());
            assert_eq!(regs, all_registers(&board));
        }
        assert!(board.device_records().is_empty() && board.frontier(false).is_empty());
    }
}
