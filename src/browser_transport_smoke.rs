//! Opt-in actual raw-key routing with generated PNG/FLV media and isolated settings.
use super::*;
pub(super) struct TransportSmoke {
    output: PathBuf, restart: bool, started: Instant, at: Instant, phase: u8,
    case: usize, round: u8, sent: bool, focus: bool, events: Vec<egui::Event>,
    ids: Vec<MediaId>, rows: Vec<serde_json::Value>,
}
impl TransportSmoke {
    pub fn new(output: PathBuf, restart: bool) -> Self {
        Self { output,restart,started:Instant::now(),at:Instant::now(),phase:0,case:0,round:0,sent:false,
            focus:true,events:vec![],ids:vec![],rows:vec![] }
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.events.clear(); raw.events.push(egui::Event::PointerGone); raw.focused=self.focus;
        raw.events.append(&mut self.events);
    }
    fn key(&mut self,key:egui::Key,modifiers:egui::Modifiers,repeat:bool) {
        for pressed in [true,false] { self.events.push(egui::Event::Key {
            key,physical_key:Some(key),pressed,repeat:repeat && pressed,modifiers }); }
        self.at=Instant::now();
    }
    fn keys(&mut self) {
        self.key(egui::Key::ArrowUp,egui::Modifiers::CTRL,false);
        for key in [egui::Key::ArrowUp,egui::Key::ArrowRight,egui::Key::M] {self.key(key,egui::Modifiers::NONE,false);}
    }
    fn go(&mut self,phase:u8) {self.phase=phase;self.at=Instant::now();self.sent=false;}
    fn capture(&mut self,b:&Browser,name:&str) {
        self.rows.push(serde_json::json!({"name":name,"phase":self.phase,"case":self.case,"round":self.round,
            "detail":b.detail.as_ref().map(|v|v.id),"volume":b.volume,"speed":b.playback_speed,"muted":b.muted,
            "fullscreen":b.video_fullscreen.is_some(),"focus":self.focus,
            "player":b.player.as_ref().map(|p|{let p=p.lock().unwrap();serde_json::json!({"speed":p.number("speed"),
                "volume":p.number("volume"),"mute":p.text("mute"),"time":p.number("time-pos"),"pause":p.text("pause"),"ready":p.has_presented_frame()})})}));
    }
    fn finish(&mut self,b:&Browser,ctx:&egui::Context,error:Option<String>) {
        self.capture(b,"finished");
        let report=serde_json::json!({"checks_pass":error.is_none(),"error":error,"version":env!("CARGO_PKG_VERSION"),
            "rows":self.rows,"restart":self.restart,"scope":"Actual egui raw keys, mpv observed speed/volume/mute/time, normal/fullscreen and generated PNG/FLV. Controlled application focus, not physical keyboard/audio certification."});
        let _=std::fs::write(&self.output,serde_json::to_vec_pretty(&report).unwrap());
        self.phase=255;ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self,b:&mut Browser,ctx:&egui::Context) {
        if self.phase==255 {return;}
        ctx.request_repaint_after(Duration::from_millis(15));
        if self.started.elapsed()>Duration::from_secs(100) {self.finish(b,ctx,Some(format!("Transport timeout phase {} case {}",self.phase,self.case)));return;}
        if let Err(error)=self.step(b,ctx) {self.finish(b,ctx,Some(error));}
    }
    fn ready(b:&Browser)->bool { !b.scan_busy && !b.prepare_dirty && b.prepared.as_ref().is_some_and(|p|
        p.snapshot.finished && p.snapshot.generation==b.generation && p.revision==b.revision) }
    fn video_ready(&self,b:&Browser,index:usize)->bool {
        b.detail.as_ref().is_some_and(|d|d.id==self.ids[index]) && b.player_media==Some(self.ids[index])
            && b.player.as_ref().is_some_and(|p|p.lock().unwrap().has_presented_frame())
    }
    fn unchanged(b:&Browser)->bool { (b.playback_speed-1.).abs()<0.001 && (b.volume-70.).abs()<0.01 && b.muted }
    fn step(&mut self,b:&mut Browser,ctx:&egui::Context)->Result<(),String> {
        use egui::{Key as K,Modifiers as M};
        match self.phase {
            0=>{
                if !Self::ready(b) {return Ok(());}
                let parent=std::fs::canonicalize(b.data_dir.parent().ok_or("No isolated data parent")?).map_err(|e|e.to_string())?;
                let target=std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).map_err(|e|e.to_string())?;
                if parent.parent()!=Some(target.as_path()) || !parent.file_name().is_some_and(|n|n.to_string_lossy().starts_with("transport-owned-"))
                    || std::fs::read(parent.join(".transport-owned")).ok().as_deref()!=Some(b"generated transport fixtures only") {return Err("Not an owned transport fixture".into());}
                let p=b.prepared.as_ref().unwrap();
                if p.snapshot.files.len()!=4 || p.snapshot.files.iter().filter(|r|r.file.kind==MediaKind::Video).count()!=2 {return Err("FLV/PNG scan counts differ from 2+2".into());}
                self.ids=["01.png","02.flv","03.png","04.FLV"].iter().map(|name|p.snapshot.files.iter()
                    .find(|r|r.file.path.file_name().is_some_and(|n|n==*name)).map(|r|r.id).ok_or("Missing generated media")).collect::<Result<Vec<_>,_>>()?;
                if self.restart {
                    if (b.playback_speed-1.25).abs()>0.001 {return Err("Playback speed not restored after restart".into());}
                    b.open_detail(self.ids[1]);self.go(20);
                }else{
                    b.volume=70.;b.muted=true;b.playback_speed=1.;b.open_detail(self.ids[1]);self.go(1);
                }
            }
            1=>{
                if !self.video_ready(b,1) {return Ok(());}
                if self.round==1 && ctx.input(|i|i.viewport().fullscreen)!=Some(true) {return Ok(());}
                b.player.as_ref().unwrap().lock().unwrap().set("pause","yes")?;
                b.request_seek(10.,true);self.case=0;self.go(2);
            }
            2=>{
                if (b.player.as_ref().unwrap().lock().unwrap().number("time-pos").unwrap_or(-10.)-10.).abs()>0.3 {return Ok(());}
                self.go(3);
            }
            3=>{
                let cases=[(K::ArrowRight,M::NONE,false,0,15.),(K::ArrowRight,M::NONE,true,0,20.),(K::ArrowLeft,M::NONE,false,0,15.),
                    (K::ArrowUp,M::NONE,false,1,75.),(K::ArrowDown,M::NONE,false,1,70.),
                    (K::ArrowUp,M::CTRL,false,2,1.25),(K::ArrowDown,M::CTRL,false,2,1.),
                    (K::M,M::NONE,false,3,0.),(K::M,M::NONE,true,3,0.),(K::M,M::NONE,false,3,1.),
                    (K::ArrowUp,M::CTRL,false,2,1.25),(K::R,M::NONE,false,2,1.),(K::Home,M::NONE,false,4,0.)];
                if self.case==cases.len() {
                    if self.round==0 {self.round=1;b.toggle_video_fullscreen();self.go(1);}
                    else {b.exit_video_fullscreen();b.open_detail(self.ids[2]);self.go(4);}
                    return Ok(());
                }
                let (key,mods,repeat,kind,value)=cases[self.case];
                if !self.sent {self.key(key,mods,repeat);self.sent=true;return Ok(());}
                if self.at.elapsed()<Duration::from_millis(170) {return Ok(());}
                if b.detail.as_ref().map(|v|v.id)!=Some(self.ids[1]) {return Err("Video key navigated to another item".into());}
                let p=b.player.as_ref().unwrap().lock().unwrap();
                let matched=match kind {
                    0=>(p.number("time-pos").unwrap_or(-10.)-value).abs()<0.35,
                    1=>(p.number("volume").unwrap_or(-10.)-value).abs()<0.1 && (b.volume as f64-value).abs()<0.1,
                    2=>(p.number("speed").unwrap_or(-10.)-value).abs()<0.001 && (b.playback_speed-value).abs()<0.001,
                    3=>p.text("mute").as_deref()==Some(if value==1. {"yes"}else{"no"}) && b.muted==(value==1.),
                    _=>p.number("time-pos").is_some_and(|v|v<1.5) && !p.paused_intent(),
                };drop(p);
                if !matched {if self.at.elapsed()>Duration::from_secs(4) {return Err(format!("Key {key:?} case {} not applied",self.case));}return Ok(());}
                self.capture(b,&format!("key-{key:?}"));self.case+=1;self.sent=false;
            }
            4=>{
                if self.at.elapsed()<Duration::from_millis(200) {return Ok(());}
                if !self.sent {
                    for (key,m) in [(K::ArrowUp,M::CTRL),(K::ArrowDown,M::CTRL),(K::ArrowUp,M::NONE),(K::ArrowDown,M::NONE),(K::M,M::NONE),(K::R,M::NONE),(K::Home,M::NONE)] {self.key(key,m,false);}
                    self.sent=true;return Ok(());
                }
                if b.detail.as_ref().map(|v|v.id)!=Some(self.ids[2]) || !Self::unchanged(b) {return Err("Video bindings leaked into photo".into());}
                self.capture(b,"photo-ignores-video-controls");self.key(K::ArrowLeft,M::NONE,false);self.go(5);
            }
            5=>{
                if b.detail.as_ref().map(|v|v.id)!=Some(self.ids[1]) {return Ok(());}
                self.capture(b,"photo-left-previous");b.open_detail(self.ids[2]);self.go(6);
            }
            6=>{
                if self.at.elapsed()<Duration::from_millis(180) {return Ok(());}
                self.key(K::ArrowRight,M::NONE,false);self.go(7);
            }
            7=>{
                if !self.video_ready(b,3) {return Ok(());}
                self.capture(b,"photo-right-next-flv1");self.focus=false;self.keys();self.go(8);
            }
            8=>{
                if self.at.elapsed()<Duration::from_millis(200) {return Ok(());}
                if !Self::unchanged(b) || b.detail.as_ref().map(|v|v.id)!=Some(self.ids[3]) {return Err("Unfocused input changed playback".into());}
                self.capture(b,"unfocused-controls-blocked");self.focus=true;b.workspace.command_open=true;self.go(9);
            }
            9=>{
                if self.at.elapsed()<Duration::from_millis(200) {return Ok(());}
                if !self.sent {self.keys();self.sent=true;return Ok(());}
                if !Self::unchanged(b) || b.detail.as_ref().map(|v|v.id)!=Some(self.ids[3]) {return Err("Command editor input changed playback".into());}
                self.capture(b,"command-editor-controls-blocked");b.workspace.command_open=false;self.go(10);
            }
            10=>{
                if self.at.elapsed()<Duration::from_millis(300) {return Ok(());}
                self.key(K::ArrowUp,M::CTRL,false);self.go(11);
            }
            11=>{
                if self.at.elapsed()<Duration::from_millis(200) {return Ok(());}
                if (b.playback_speed-1.25).abs()>0.001 {return Err("Controls did not resume after editor closed".into());}
                self.capture(b,"save-selected-speed");self.finish(b,ctx,None);
            }
            20=>{
                if !self.video_ready(b,1) {return Ok(());}
                if b.player.as_ref().unwrap().lock().unwrap().number("speed").is_some_and(|v|(v-1.25).abs()<0.001) {
                    self.capture(b,"restart-applies-selected-speed-to-mpv");self.finish(b,ctx,None);
                }
            }
            _=>{}
        }
        Ok(())
    }
}
