use std::cmp::Ordering;
use std::io::{BufReader, Read};

use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::data::{Dim, InHandle, Pos, Viewport};
use crate::meta::MetaHeader;
use crate::spec::BYTES_PER_PIXEL;
use crate::TranscodeError;

#[derive(Debug, Deserialize)]
pub struct RuleRectJson {
    // [x,y, width, height] -- cpy src
    pub src: [u32; 4],
    // [x,y] -- cpy dst
    pub dest: [u32; 2],
    // -- opacity of composited rect
    pub alpha: f32,
    // -- z-index of compositing order
    pub z: u32,
}

#[derive(Debug, Deserialize)]
pub struct RuleDataJson {
    // [width, height]
    pub size: [u32; 2],
    pub rects: Vec<RuleRectJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RuleRect {
    // [x,y, width, height] -- cpy src
    pub(crate) src: Viewport,
    // [x,y] -- cpy dst
    pub(crate) dest: Pos,
    // -- opacity of composited rect
    pub(crate) alpha: f32,
    // -- z-index of compositing order
    pub(crate) z: u32,
}

impl From<RuleRectJson> for RuleRect {
    fn from(value: RuleRectJson) -> Self {
        let [x, y, width, height] = value.src;
        let src = Viewport {
            ul: Pos { x, y },
            dim: Dim { width, height },
        };
        let [x, y] = value.dest;
        let dest = Pos { x, y };
        Self {
            src,
            dest,
            alpha: value.alpha,
            z: value.z,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RuleData {
    size: Dim,
    rects: Vec<RuleRect>,
}

impl From<RuleDataJson> for RuleData {
    fn from(value: RuleDataJson) -> Self {
        let [width, height] = value.size;
        let rects: Vec<RuleRect> = value.rects.into_iter().map(|j| j.into()).collect();
        Self {
            size: Dim { width, height },
            rects,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PartialRules {
    rules: RuleData,
    pub occlude_opaque_layers: bool,
    debug: bool,
}

impl PartialRules {
    pub fn build(self, meta_header: MetaHeader) -> Result<Rules, TranscodeError> {
        let blit_ctx = BlitContext::new(meta_header.dim, self.rules.size)?;
        let mut rules = Rules {
            rules: self.rules,
            blit_ctx,
            debug: self.debug,
        };
        rules.pre_process_rules(self.occlude_opaque_layers)?;
        Ok(rules)
    }
}

pub struct Rules {
    rules: RuleData,
    blit_ctx: BlitContext,
    debug: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RuleOpts {
    pub occlude_opaque_layers: bool,
    pub debug: bool,
}

impl RuleOpts {
    // pub fn parse_rules_file<FN>(
    //     self,
    //     meta_header: MetaHeader,
    //     file_name: FN,
    // ) -> Result<Rules, TranscodeError>
    //     where
    //         FN: AsRef<Path>,
    // {
    //     let rdr = File::open(file_name)?;
    //     self.parse_rules_rdr(meta_header, rdr)
    // }
    // pub fn with_json_file<FN>(
    //     self,
    //     file_name: FN,
    // ) -> Result<PartialRules, TranscodeError>
    //     where
    //         FN: AsRef<Path>,
    // {
    //     let rdr = File::open(file_name)?;
    //     self.parse_rules_rdr(rdr)
    // }

    // pub fn parse_rules_rdr(
    //     self,
    //     meta_header: MetaHeader,
    //     rdr: impl Read,
    // ) -> Result<Rules, TranscodeError> {
    //     let buf_rdr = BufReader::with_capacity(16384, rdr);
    //     self.parse_rules_buf_rdr(meta_header, buf_rdr)
    // }

    // pub fn parse_rules_buf_rdr<F>(
    //     self,
    //     meta_header: MetaHeader,
    //     buf_rdr: BufReader<F>,
    // ) -> Result<Rules, TranscodeError>
    //     where
    //         F: Read,
    // {
    //     let json: RuleDataJson = serde_json::from_reader(buf_rdr)?;
    //     let rules: RuleData = json.into();
    //     let blit_ctx = BlitContext::new(meta_header.dim, rules.size)?;
    //     let mut rules = Rules {
    //         rules,
    //         blit_ctx,
    //         debug: self.debug,
    //     };
    //
    //     rules.pre_process_rules(self.occlude_opaque_layers)?;
    //     Ok(rules)
    // }
    pub fn with_json_in_handle(self, in_handle: InHandle) -> Result<PartialRules, TranscodeError> {
        match in_handle {
            InHandle::File(u) => self.with_json_file_sync(u.as_str()),
            _ => Err(TranscodeError::MissingRules),
        }
    }

    pub fn with_json_file_sync(self, file: &str) -> Result<PartialRules, TranscodeError> {
        let mut fh = std::fs::File::open(file).map_err(|_| TranscodeError::FileTooShort)?;
        let mut buf = Vec::new();
        let _sz = fh
            .read_to_end(&mut buf)
            .map_err(|_| TranscodeError::FileTooShort)?;
        let json: RuleDataJson = serde_json::from_slice(buf.as_slice())?;
        self.with_json(json)
    }
    pub async fn with_json_file_async(self, file: &str) -> Result<PartialRules, TranscodeError> {
        let mut fh = tokio::fs::File::open(file)
            .await
            .map_err(|_| TranscodeError::FileTooShort)?;
        let mut buf = Vec::new();
        let _sz = fh
            .read_to_end(&mut buf)
            .await
            .map_err(|_| TranscodeError::FileTooShort)?;
        let json: RuleDataJson = serde_json::from_slice(buf.as_slice())?;
        self.with_json(json)
    }

    pub fn with_json_rdr<F>(self, rdr: impl Read) -> Result<PartialRules, TranscodeError>
    where
        F: Read,
    {
        let buf_rdr = BufReader::with_capacity(16384, rdr);
        self.with_json_buf_rdr(buf_rdr)
    }

    pub fn with_json_buf_rdr<F>(self, buf_rdr: BufReader<F>) -> Result<PartialRules, TranscodeError>
    where
        F: Read,
    {
        let json: RuleDataJson = serde_json::from_reader(buf_rdr)?;
        let rules: RuleData = json.into();
        Ok(PartialRules {
            rules: rules,
            occlude_opaque_layers: self.occlude_opaque_layers,
            debug: self.debug,
        })
    }
    // pub fn parse_rules_json<F>(
    //     self,
    //     meta_header: MetaHeader,
    //     json: RuleDataJson,
    // ) -> Result<Rules, TranscodeError>
    //     where
    //         F: Read,
    // {
    //     let rules: RuleData = json.into();
    //     let blit_ctx = BlitContext::new(meta_header.dim, rules.size)?;
    //     let mut rules = Rules {
    //         rules,
    //         blit_ctx,
    //         debug: self.debug,
    //     };
    //
    //     rules.pre_process_rules(self.occlude_opaque_layers)?;
    //     Ok(rules)
    // }
    pub fn with_json(self, json: RuleDataJson) -> Result<PartialRules, TranscodeError> {
        let rules: RuleData = json.into();
        Ok(PartialRules {
            rules: rules,
            occlude_opaque_layers: self.occlude_opaque_layers,
            debug: self.debug,
        })
    }
}

impl Rules {
    // Looking at the sample data alpha channels, this will never do anything
    // But front-to-back optimizations often can be massive time savers for a relatively
    // inexpensive pre-check
    fn eliminate_occlusions(&mut self) -> Result<(), TranscodeError> {
        let area = self.rules.size.calc_area(1); // bool is 1 byte
        let mut zbuf: Vec<bool> = Vec::with_capacity(area as usize);
        let mut to_del = Vec::new();

        // walk in reverse order to achieve front-to-back for occlusion detection
        for (idx, rule) in self.rules.rects.iter().enumerate().rev() {
            if rule.alpha > 1.0 {
                return Err(TranscodeError::InvalidAlpha);
            }
            if (1.0 - rule.alpha) > f32::EPSILON {
                // non-opaque
                continue;
            }
            let mut any_found = false;
            let dim = rule.src.dim;
            let ul = rule.dest;
            let row_stride = self.rules.size.width;
            let mut pos = ul.y * row_stride + ul.x;
            for _ in 0..dim.height {
                let mut lpos = pos as usize;
                // TODO, vectorize
                for _ in 0..dim.width {
                    if !zbuf[lpos] {
                        any_found = true;
                        zbuf[lpos] = true
                    }
                    lpos += 1;
                }
                if !any_found {
                    to_del.push(idx)
                }
                pos += row_stride
            }
        }

        // see if any occlusions were found
        if !to_del.is_empty() {
            println!("Note, occluding the following rules: {to_del:?}");
            const RESORT_THRESHOLD: usize = 8;
            if to_del.len() < RESORT_THRESHOLD {
                // cheaper / simpler to just remove it
                for idx_to_remove in to_del.into_iter() {
                    // O(k * n) cost
                    self.rules.rects.remove(idx_to_remove);
                }
            } else {
                // may be cheaper to swap-pop, then re-sort

                for to_del_idx in to_del.into_iter() {
                    // take last entry and swap with target, then pop last entry
                    // O(1) cost
                    self.rules.rects.swap_remove(to_del_idx);
                }
                // O(n lg(n)) cost
                self.sort_rules()
            }
        }
        Ok(())
    }

    fn sort_rules(&mut self) {
        if self.debug {
            // ignore
        }
        self.rules.rects.sort_by(|a, b| match a.z.cmp(&b.z) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => match a.src.ul.x.cmp(&b.src.ul.x) {
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
                Ordering::Equal => a.src.ul.y.cmp(&b.src.ul.y),
            },
        });
    }

    fn pre_process_rules(&mut self, occlude_opaque_layers: bool) -> Result<(), TranscodeError> {
        if self.rules.rects.len() > u32::MAX as usize {
            return Err(TranscodeError::TooManyRules);
        }
        self.sort_rules();

        // This winds up not being used in the sample data
        // but could be a time saver
        if occlude_opaque_layers {
            self.eliminate_occlusions()?;
        }
        Ok(())
    }

    pub(crate) fn get_tgt_dim(&self) -> Dim {
        self.rules.size
    }

    pub(crate) fn process_frame(
        &self,
        _fr_num: u32,
        src_frame: &[u8],
        tgt_frame: &mut [u8],
    ) -> Result<(), TranscodeError> {
        // clear buffer
        tgt_frame.iter_mut().for_each(|m| *m = 0);

        for rule_rect in self.rules.rects.iter() {
            self.blit_ctx.blit(
                src_frame,
                tgt_frame,
                rule_rect.src.ul,
                rule_rect.dest,
                rule_rect.src.dim,
                rule_rect.alpha,
            );
        }
        Ok(())
    }
}

struct BlitContext {
    // pre-computed src row-over-row stride
    src_width: u32,
    // pre-computed tgt row-over-row stride
    tgt_width: u32,
}

// This is the core rule-copy-engine; the inner-most loop
// We wish to advance on a stride and use a tight inner loop that LLVM can instrinsic-way
impl BlitContext {
    fn new(src_dim: Dim, tgt_dim: Dim) -> Result<Self, TranscodeError> {
        let src_width = BYTES_PER_PIXEL * src_dim.width;
        let tgt_width = BYTES_PER_PIXEL * tgt_dim.width;
        if src_width == 0 {
            Err(TranscodeError::InvalidHeader)
        } else if tgt_width == 0 {
            Err(TranscodeError::InvalidRules)
        } else {
            Ok(Self {
                src_width,
                tgt_width,
            })
        }
    }

    fn blit(&self, src: &[u8], tgt: &mut [u8], src_ul: Pos, tgt_ul: Pos, cpy_dim: Dim, alpha: f32) {
        let src_stride_u32 = self.src_width;
        let tgt_stride_u32 = self.tgt_width;
        let src_stride = src_stride_u32 as usize;
        let tgt_stride = tgt_stride_u32 as usize;
        let mut src_pos = (src_stride_u32 * src_ul.y + src_ul.x * BYTES_PER_PIXEL) as usize;
        let mut tgt_pos = (tgt_stride_u32 * tgt_ul.y + tgt_ul.x * BYTES_PER_PIXEL) as usize;
        let cpy_len = cpy_dim.calc_stride(BYTES_PER_PIXEL) as usize;
        let alpha_bg = 1.0 - alpha;
        let alpha_fg = alpha;
        for _ in 0..cpy_dim.height {
            let src_slice = &src[src_pos..src_pos + cpy_len];
            let dst_slice = &mut tgt[tgt_pos..tgt_pos + cpy_len];

            // SSE / AVX / AVX2 turns this into melted pudding
            // see Godbolt
            for (s, d) in src_slice.iter().zip(dst_slice.iter_mut()) {
                let old_d = *d;
                *d = ((*s as f32) * alpha_fg + (old_d as f32) * alpha_bg) as u8;
            }
            src_pos += src_stride;
            tgt_pos += tgt_stride;
        }
    }
}

#[test]
fn test_stats1() {
    println!("start");
    let ro = RuleOpts {
        occlude_opaque_layers: false,
        debug: false,
    };
    let dim = Dim {
        width: 3840,
        height: 2160,
    };
    let _mh = MetaHeader {
        dim: dim,
        num_frames: 1,

        frame_size: dim.calc_area(3) as usize,
    };
    let rules = ro.with_json_file_sync("input/rules.json").unwrap();
    let num_rows_per_strip = 1_000_000 / 3 / dim.width;
    #[derive(Debug)]
    struct ZRec {
        rules: Vec<RuleRect>,
    }
    let z_0 = rules.rules.rects[0].z;
    let z_n = rules.rules.rects[rules.rules.rects.len() - 1].z;
    let num_rows_per_strip_m1 = num_rows_per_strip - 1;
    let num_pages = (dim.height + num_rows_per_strip_m1) / num_rows_per_strip;

    #[derive(Debug)]
    struct PageInfo {
        zrects: Vec<ZRec>,
    }
    let mut pages = vec![];
    // let mut bmap = BTreeMap::new();
    for page in 0..num_pages {
        let min_row = page * num_rows_per_strip;
        let max_row_excl = dim.height.min(min_row + num_rows_per_strip);

        let mut z_rules: Vec<ZRec> = (0..=z_n)
            .into_iter()
            .map(|_| ZRec { rules: vec![] })
            .collect();
        for z in z_0..=z_n {
            let z_rules = &mut z_rules[z as usize].rules;

            for rule in rules.rules.rects.iter() {
                if rule.z != z {
                    continue;
                }
                let dest_y0 = rule.dest.y;
                let dest_y1_excl = dest_y0 + rule.src.dim.height;
                if dest_y0 < max_row_excl && dest_y1_excl > min_row {
                    // intersects
                    let dest_start_row = min_row.max(dest_y0);
                    let dest_end_row_excl = max_row_excl.min(dest_y1_excl);
                    let num_rows = dest_end_row_excl - dest_start_row;
                    let num_rows_skipped = dest_start_row - dest_y0;
                    let src_y = rule.src.ul.y + num_rows_skipped;
                    let src = Viewport {
                        ul: Pos {
                            x: rule.src.ul.x,
                            y: src_y,
                        },
                        dim: Dim {
                            width: rule.src.dim.width,
                            height: num_rows,
                        },
                    };

                    let rr = RuleRect {
                        src,
                        dest: Pos {
                            x: rule.dest.x,
                            y: dest_start_row,
                        },
                        ..*rule
                    };
                    z_rules.push(rr);
                }
            }
            z_rules.sort_by(|a, b| match a.dest.y.cmp(&b.dest.y) {
                Ordering::Equal => a.dest.x.cmp(&b.dest.x),
                x => x,
            });
        }
        pages.push(PageInfo { zrects: z_rules });
    }
    let mut tot = 0;
    for (idx, pi) in pages.iter().enumerate() {
        println!("page {idx}");
        for (idx, zi) in pi.zrects.iter().enumerate() {
            if !zi.rules.is_empty() {
                print!("z {idx}: ");
                for r in zi.rules.iter() {
                    print!(
                        "{}+{}:{}-{} ",
                        r.dest.y,
                        r.src.dim.height,
                        r.dest.x,
                        r.dest.x + r.src.dim.width
                    );
                    tot += 1;
                }
                println!();
            }
        }
    }
    println!("tot: {tot}");
}

#[test]
fn test_stats() {
    println!("start");
    let ro = RuleOpts {
        occlude_opaque_layers: false,
        debug: false,
    };
    let dim = Dim {
        width: 3840,
        height: 2160,
    };
    let _mh = MetaHeader {
        dim: dim,
        num_frames: 1,

        frame_size: dim.calc_area(3) as usize,
    };
    let rules = ro.with_json_file_sync("input/rules.json").unwrap();

    const SCALE: u32 = 512;
    const SCALEM1: u32 = SCALE - 1;
    let n_cols = (rules.rules.size.width + SCALEM1) / SCALE;
    let n_rows = (rules.rules.size.height + SCALEM1) / SCALE;
    let n_sz = (n_cols * n_rows) as usize;
    let mut quad_src_dx = vec![0u32; n_sz];
    let mut quad_dst_dx = vec![0u32; n_sz];
    let mut quad_src_ul = vec![0u32; n_sz];
    let mut quad_dst_ul = vec![0u32; n_sz];

    let mut quad_intra = vec![0u32; n_sz];

    for r in rules.rules.rects.iter() {
        {
            let st = r.dest.y;
            let y0 = st / SCALE;

            let st = r.dest.x;
            let x0 = st / SCALE;
            let idx = y0 * n_cols + x0;
            quad_dst_ul[idx as usize] += 1;
        }
        {
            let st = r.src.ul.y;
            let y0 = st / SCALE;

            let st = r.src.ul.x;
            let x0 = st / SCALE;
            let idx = y0 * n_cols + x0;
            quad_src_ul[idx as usize] += 1;
        }
        {
            let st = r.src.ul.y;
            let ed = r.src.ul.y + r.src.dim.height;
            let y0 = st / SCALE;
            let y1 = ed / SCALE;

            let st = r.src.ul.x;
            let ed = r.src.ul.x + r.src.dim.width;
            let x0 = st / SCALE;
            let x1 = ed / SCALE;
            let dx = x1 - x0;
            let dy = y1 - y0;
            let idx = dy * n_cols + dx;
            quad_src_dx[idx as usize] += 1;
        }
        {
            let st = r.dest.y;
            let ed = r.dest.y + r.src.dim.height;
            let y0 = st / SCALE;
            let y1 = ed / SCALE;

            let st = r.dest.x;
            let ed = r.dest.x + r.src.dim.width;
            let x0 = st / SCALE;
            let x1 = ed / SCALE;
            let dx = x1 - x0;
            let dy = y1 - y0;
            let idx = dy * n_cols + dx;
            quad_dst_dx[idx as usize] += 1;
        }
        loop {
            let st = r.src.ul.y / SCALE;
            let ed = r.src.ul.y + r.src.dim.height;
            let ed = ed / SCALE;
            if ed != st {
                break;
            }
            let st = r.dest.y / SCALE;
            if ed != st {
                break;
            }

            let ed = r.dest.y + r.src.dim.height;
            let ed = ed / SCALE;
            if ed != st {
                break;
            }
            let y = ed;

            let st = r.src.ul.x / SCALE;
            let ed = r.src.ul.x + r.src.dim.width;
            let ed = ed / SCALE;
            if ed != st {
                break;
            }
            let st = r.dest.x / SCALE;
            if ed != st {
                break;
            }
            let ed = r.dest.x + r.src.dim.width;
            let ed = ed / SCALE;
            if ed != st {
                break;
            }
            let x = ed;

            let idx = y * n_cols + x;
            quad_intra[idx as usize] += 1;
            break;
        }
    }
    if true {
        println!("{:?} {}x{}", rules.rules.size, n_rows, n_cols);
        println!("Num Quad overruns:");
        print!("row: XX: ");
        for col in 0..n_cols {
            print!(" {col:5}");
        }
        println!("");
        let render = |v: &[u32], nm| {
            println!("{nm:2}");

            for row in 0..n_rows {
                print!("row: {row:2}: ");
                for col in 0..n_cols {
                    let v = v[(row * n_cols + col) as usize];
                    print!(" {v:5}");
                }
                println!("");
            }
        };
        render(&quad_src_dx, "src_dx");
        render(&quad_dst_dx, "dst_dx");
        render(&quad_src_ul, "src_ul");
        render(&quad_dst_ul, "dst_ul");
        render(&quad_intra, "intra");
    }

    // println!("items: {:?}", self.rules);
}
