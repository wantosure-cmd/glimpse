//! Opt-in native entry-point and restart checks; only generated owned fixtures.
use super::*;

pub(super) struct DailySmoke {
    pub delete_action: Option<Rect>,
    output: PathBuf,
    restart: bool,
    scan_exit: bool,
    started: Instant,
    at: Instant,
    phase: u8,
    case: usize,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    fixture: Option<PathBuf>,
    expected: Vec<PathBuf>,
    pending: Option<serde_json::Value>,
    generation: u64,
}
impl DailySmoke {
    pub fn new(output: PathBuf, restart: bool, scan_exit: bool) -> Self {
        Self {delete_action:None, output,restart,scan_exit,started:Instant::now(),at:Instant::now(),
            phase:0,case:0,events:vec![],rows:vec![],fixture:None,expected:vec![],pending:None,generation:0}
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.events.clear();
        raw.events.push(egui::Event::PointerGone);
        raw.focused=true;
        raw.events.append(&mut self.events);
    }
    fn key(&mut self,key:egui::Key) {
        for pressed in [true,false] {self.events.push(egui::Event::Key {
            key,physical_key:Some(key),pressed,repeat:false,modifiers:Default::default()});}
    }
    fn click(&mut self,pos:Pos2,button:egui::PointerButton) {
        self.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true,false] {self.events.push(egui::Event::PointerButton {
            pos,button,pressed,modifiers:Default::default()});}
    }
    fn go(&mut self,phase:u8) {self.phase=phase;self.at=Instant::now();}
    fn ready(b:&Browser) -> bool {
        !b.scan_busy && !b.prepare_dirty && b.prepared.as_ref().is_some_and(|p|
            p.snapshot.finished && p.snapshot.generation==b.generation && p.revision==b.revision)
    }
    fn id(b:&Browser,name:&str)->Result<MediaId,String> {
        b.prepared.as_ref().and_then(|p|p.snapshot.files.iter().find(|r|
            r.file.path.file_name().is_some_and(|n|n==name))).map(|r|r.id)
            .ok_or_else(||format!("Missing generated sample {name}"))
    }
    fn capture(&mut self,b:&Browser,name:&str) {
        self.rows.push(serde_json::json!({"name":name,"case":self.case,"phase":self.phase,
            "generation":b.generation,"scan_busy":b.scan_busy,"detail":b.detail.as_ref().map(|v|v.id),
            "root":b.watch_root,"fullscreen":b.video_fullscreen.is_some(),
            "trash":b.trash_diagnostic_for_smoke()}));
    }
    fn finish(&mut self,b:&mut Browser,ctx:&egui::Context,error:Option<String>) {
        self.capture(b,"finished");
        if error.is_some() {b.pending_trash=None;}
        let report=serde_json::json!({"checks_pass":error.is_none(),"error":error,
            "version":env!("CARGO_PKG_VERSION"),"restart":self.restart,"rows":self.rows,
            "scope":"Native injected input through real menus/Delete, generated fixture only. Pending deletion targets checked and undone. Separate-process restart; not physical input, real network or second-machine certification."});
        let _=std::fs::write(&self.output,serde_json::to_vec_pretty(&report).unwrap());
        self.phase=255;ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self,b:&mut Browser,ctx:&egui::Context) {
        if self.phase==255 {return;}
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed()>Duration::from_secs(90) {
            self.finish(b,ctx,Some(format!("Daily test timed out at phase {}, case {}",self.phase,self.case)));return;
        }
        if let Err(e)=self.step(b,ctx) {self.finish(b,ctx,Some(e));}
    }
    fn step(&mut self,b:&mut Browser,ctx:&egui::Context)->Result<(),String> {
        let elapsed=self.at.elapsed();
        match self.phase {
            0=>{
                if !Self::ready(b) {return Ok(());}
                let fixture=std::fs::canonicalize(b.data_dir.parent().ok_or("Missing owned parent")?).map_err(|e|e.to_string())?;
                let target=std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).map_err(|e|e.to_string())?;
                if fixture.parent()!=Some(target.as_path()) || !fixture.file_name().is_some_and(|n|n.to_string_lossy().starts_with("daily-owned-"))
                    || std::fs::read(fixture.join(".daily-owned")).ok().as_deref()!=Some(b"generated daily fixtures only") {
                    return Err("Refuse daily test outside marked generated fixture".into());
                }
                self.fixture=Some(fixture.clone());
                let root=std::fs::canonicalize(b.watch_root.as_ref().ok_or("No restored root")?).map_err(|e|e.to_string())?;
                let expected_root=fixture.join(if self.restart && self.scan_exit {"A"}else{"B"});
                if root!=expected_root {return Err(format!("Startup did not restore expected root: {root:?}, expected {expected_root:?}"));}
                if self.scan_exit {
                    if self.restart {
                        if b.prepared.as_ref().unwrap().snapshot.files.len()!=512 || b.pending_trash.is_some() {return Err("Invalid restored scan root".into());}
                        self.capture(b,"restart-restores-directory-closed-during-scan");
                    } else {
                        b.navigate(fixture.join("A"),false);
                        if !b.scan_busy || b.snapshot.is_some() {return Err("Exit was not during pending scan".into());}
                        self.capture(b,"exit-while-new-root-scan-is-pending");
                    }
                    self.finish(b,ctx,None);return Ok(());
                }
                if self.restart {
                    if b.pending_trash.is_some() || b.trash_operation.is_some() || b.detail.is_some() || !b.selection.is_empty()
                        || b.settings.sort!=SortField::Size || !b.settings.descending {
                        return Err("Restart restored invalid transient state or lost saved settings".into());
                    }
                    let id=Self::id(b,"01.png")?;
                    let p=b.prepared.as_ref().unwrap();let path=&p.snapshot.files[p.lookup[&id]].file.path;
                    if !b.features.favorites.paths.contains(path) {return Err("Favorite lost after process restart".into());}
                    let previous:serde_json::Value=serde_json::from_slice(&std::fs::read(fixture.join("saved-identity.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
                    if previous["id"]!=serde_json::json!(id) {return Err("Indexed media identity changed after restart".into());}
                    self.capture(b,"restart-restores-root-settings-favorite-and-id");
                    self.finish(b,ctx,None);return Ok(());
                }
                b.navigate(fixture.join("A"),false);
                self.generation=b.generation;
                if !b.scan_busy {return Err("A scan was not queued".into());}
                self.capture(b,"scan-A-queued");
                // Switch while the first scan is busy, before accepting its results.
                b.navigate(fixture.join("B"),false);
                self.capture(b,"switch-to-B-during-scan");self.go(1);
            }
            1=>{
                if !Self::ready(b) {return Ok(());}
                let p=b.prepared.as_ref().unwrap();
                if b.generation<=self.generation || p.snapshot.files.len()!=4 || p.snapshot.files.iter().any(|r|!r.file.path.starts_with(self.fixture.as_ref().unwrap().join("B"))) {
                    return Err("Old scan contaminated B".into());
                }
                self.capture(b,"B-settled-without-A-results");self.go(10);
            }
            10=>{
                if !Self::ready(b) {return Ok(());}
                if self.case==8 {self.go(30);return Ok(());}
                b.close_detail();b.selection=HashSet::from([Self::id(b,"02.png")?]);
                if self.case==1 {b.selection=HashSet::from([Self::id(b,"01.png")?]);}
                self.delete_action=None;
                let name=if self.case<2 {"02.png"} else if self.case<4 {"01.png"} else {"03.mov"};
                self.expected=vec![self.fixture.as_ref().unwrap().join("B").join(name)];
                if self.case==0 {
                    b.selection.insert(Self::id(b,"01.png")?);
                    self.expected.push(self.fixture.as_ref().unwrap().join("B/01.png"));
                }
                if self.case>=2 {b.open_detail(Self::id(b,name)?);}
                if self.case>=6 {b.toggle_video_fullscreen();}
                self.go(11);
            }
            11=>{
                if elapsed<Duration::from_millis(450) {return Ok(());}
                if self.case>=4 && !b.player.as_ref().is_some_and(|p|p.lock().unwrap().has_presented_frame()) {return Ok(());}
                if self.case>=6 && ctx.input(|i|i.viewport().fullscreen)!=Some(true) {return Ok(());}
                if self.case%2==0 {self.key(egui::Key::Delete);self.go(13);}
                else {
                    let pos=if self.case==1 {*b.list_regions.get(&Self::id(b,"02.png")?).ok_or("List target not visible")?}
                        else {b.detail_layout.canvas.ok_or("No detail canvas")?};
                    self.click(pos.center(),egui::PointerButton::Secondary);self.go(12);
                }
            }
            12=>{
                if elapsed<Duration::from_millis(150) {return Ok(());}
                if let Some(rect)=self.delete_action.filter(|_|egui::Popup::is_any_open(ctx)) {
                    self.click(rect.center(),egui::PointerButton::Primary);self.go(13);
                } else if elapsed>Duration::from_secs(2) {return Err("Delete item did not appear in context menu".into());}
            }
            13=>{
                if elapsed<Duration::from_millis(180) {return Ok(());}
                let d=b.trash_diagnostic_for_smoke();
                let Some(paths)=d["pending"]["paths"].as_array() else {return Err(format!("Entry did not queue undo: case {}, {d}",self.case));};
                let mut actual=paths.iter().map(|v|PathBuf::from(v.as_str().unwrap())).map(|p|std::fs::canonicalize(p).unwrap()).collect::<Vec<_>>();
                let mut expected=self.expected.clone();actual.sort();expected.sort();
                if actual!=expected || b.trash_operation.is_some() || b.trash_counter!=0 {return Err(format!("Wrong delete targets at entry {}: {actual:?}, expected {expected:?}",self.case));}
                self.pending=Some(d["pending"].clone());self.capture(b,"entry-queues-exact-targets");
                self.key(egui::Key::Delete);self.go(14);
            }
            14=>{
                if elapsed<Duration::from_millis(160) {return Ok(());}
                let d=b.trash_diagnostic_for_smoke();let previous=self.pending.as_ref().unwrap();
                if d["pending"]["paths"]!=previous["paths"] || d["pending"]["age_ms"].as_u64()<previous["age_ms"].as_u64() || b.trash_counter!=0 {
                    return Err("Repeated Delete replaced or submitted pending targets".into());
                }
                self.click(b.trash_undo_button.ok_or("Missing undo control")?.center(),egui::PointerButton::Primary);self.go(15);
            }
            15=>{
                if elapsed<Duration::from_millis(160) {return Ok(());}
                if b.pending_trash.is_some() || b.trash_operation.is_some() || b.trash_counter!=0 {return Err("Undo failed".into());}
                if self.case>=2 && b.detail.as_ref().map(|v|v.id)!=Some(Self::id(b,if self.case<4 {"01.png"}else{"03.mov"})?) {return Err("Undo click changed detail".into());}
                if self.case>=6 && b.video_fullscreen.is_none() {return Err("Undo click exited fullscreen".into());}
                self.capture(b,"entry-undone-without-side-effects");self.case+=1;self.go(10);
            }
            30=>{
                b.close_detail();b.selection=HashSet::from([Self::id(b,"02.png")?]);b.begin_trash();
                self.pending=Some(b.trash_diagnostic_for_smoke()["pending"].clone());
                b.navigate(self.fixture.as_ref().unwrap().join("A"),false);self.go(31);
            }
            31=>{
                if !Self::ready(b) {return Ok(());}
                let d=b.trash_diagnostic_for_smoke();
                if d["pending"]["paths"]!=self.pending.as_ref().unwrap()["paths"] {return Err("Navigation changed pending delete paths".into());}
                self.capture(b,"pending-targets-survive-root-change");
                self.click(b.trash_undo_button.ok_or("Undo missing after root change")?.center(),egui::PointerButton::Primary);self.go(32);
            }
            32=>{
                if elapsed<Duration::from_millis(180) {return Ok(());}
                if b.pending_trash.is_some() {return Err("Undo after navigation failed".into());}
                b.navigate(self.fixture.as_ref().unwrap().join("B"),false);self.go(33);
            }
            33=>{
                if !Self::ready(b) {return Ok(());}
                b.settings.filter=media_browser::view::MediaFilter::Videos;b.prepare_dirty=true;b.prepare_after=Instant::now();self.go(34);
            }
            34=>{
                if !Self::ready(b) {return Ok(());}
                let p=b.prepared.as_ref().unwrap();
                if p.view.sequence.len()!=2 || p.view.sequence.iter().any(|id|p.snapshot.files[p.lookup[id]].file.kind!=MediaKind::Video) {return Err("Video filter sequence wrong".into());}
                b.open_detail(Self::id(b,"03.mov")?);self.key(egui::Key::ArrowRight);self.go(35);
            }
            35=>{
                if elapsed<Duration::from_millis(350) {return Ok(());}
                if b.detail.as_ref().map(|v|v.id)!=Some(Self::id(b,"04.mov")?) {return Err("Detail navigation did not follow filtered sequence".into());}
                self.capture(b,"filtered-detail-next-video");b.close_detail();
                b.settings.filter=media_browser::view::MediaFilter::All;b.settings.sort=SortField::Size;b.settings.descending=true;
                b.prepare_dirty=true;b.prepare_after=Instant::now();self.go(36);
            }
            36=>{
                if !Self::ready(b) {return Ok(());}
                let id=Self::id(b,"01.png")?;let p=b.prepared.as_ref().unwrap();let path=p.snapshot.files[p.lookup[&id]].file.path.clone();
                b.toggle_favorite(&path);
                std::fs::write(self.fixture.as_ref().unwrap().join("saved-identity.json"),serde_json::to_vec(&serde_json::json!({"id":id})).unwrap()).map_err(|e|e.to_string())?;
                b.selection=HashSet::from([Self::id(b,"02.png")?]);b.begin_trash();
                if b.pending_trash.is_none() {return Err("No pending deletion before exit".into());}
                self.capture(b,"exit-during-unsubmitted-delete");self.finish(b,ctx,None);
            }
            _=>{}
        }
        Ok(())
    }
}
