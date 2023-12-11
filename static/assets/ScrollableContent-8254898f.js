import{a as F,n as x,d as V,a3 as X,o as M,c as T,i as D,M as k,F as G,A as I,L as O,C as q,a0 as U}from"./index-01d98e79.js";const mt=F("XIcon",[["path",{d:"M18 6 6 18",key:"1bl5f8"}],["path",{d:"m6 6 12 12",key:"d8bk6v"}]]);var j=globalThis&&globalThis.__awaiter||function(h,t,e,i){function r(n){return n instanceof e?n:new e(function(o){o(n)})}return new(e||(e=Promise))(function(n,o){function s(a){try{l(i.next(a))}catch(d){o(d)}}function c(a){try{l(i.throw(a))}catch(d){o(d)}}function l(a){a.done?n(a.value):r(a.value).then(s,c)}l((i=i.apply(h,t||[])).next())})};function Y(h,t){return j(this,void 0,void 0,function*(){const e=new AudioContext({sampleRate:t});return e.decodeAudioData(h).finally(()=>e.close())})}function J(h){const t=h[0];if(t.some(e=>e>1||e<-1)){const e=t.length;let i=0;for(let r=0;r<e;r++){const n=Math.abs(t[r]);n>i&&(i=n)}for(const r of h)for(let n=0;n<e;n++)r[n]/=i}return h}function Q(h,t){return typeof h[0]=="number"&&(h=[h]),J(h),{duration:t,length:h[0].length,sampleRate:h[0].length/t,numberOfChannels:h.length,getChannelData:e=>h==null?void 0:h[e],copyFromChannel:AudioBuffer.prototype.copyFromChannel,copyToChannel:AudioBuffer.prototype.copyToChannel}}const $={decode:Y,createBuffer:Q};var W=globalThis&&globalThis.__awaiter||function(h,t,e,i){function r(n){return n instanceof e?n:new e(function(o){o(n)})}return new(e||(e=Promise))(function(n,o){function s(a){try{l(i.next(a))}catch(d){o(d)}}function c(a){try{l(i.throw(a))}catch(d){o(d)}}function l(a){a.done?n(a.value):r(a.value).then(s,c)}l((i=i.apply(h,t||[])).next())})};function K(h,t){return W(this,void 0,void 0,function*(){if(!h.body||!h.headers)return;const e=h.body.getReader(),i=Number(h.headers.get("Content-Length"))||0;let r=0;const n=s=>W(this,void 0,void 0,function*(){r+=(s==null?void 0:s.length)||0;const c=Math.round(r/i*100);t(c)}),o=()=>W(this,void 0,void 0,function*(){let s;try{s=yield e.read()}catch{return}s.done||(n(s.value),yield o())});o()})}function Z(h,t,e){return W(this,void 0,void 0,function*(){const i=yield fetch(h,e);return K(i.clone(),t),i.blob()})}const tt={fetchBlob:Z};class L{constructor(){this.listeners={},this.on=this.addEventListener,this.un=this.removeEventListener}addEventListener(t,e,i){if(this.listeners[t]||(this.listeners[t]=new Set),this.listeners[t].add(e),i!=null&&i.once){const r=()=>{this.removeEventListener(t,r),this.removeEventListener(t,e)};return this.addEventListener(t,r),r}return()=>this.removeEventListener(t,e)}removeEventListener(t,e){var i;(i=this.listeners[t])===null||i===void 0||i.delete(e)}once(t,e){return this.on(t,e,{once:!0})}unAll(){this.listeners={}}emit(t,...e){this.listeners[t]&&this.listeners[t].forEach(i=>i(...e))}}class et extends L{constructor(t){super(),this.isExternalMedia=!1,t.media?(this.media=t.media,this.isExternalMedia=!0):this.media=document.createElement("audio"),t.mediaControls&&(this.media.controls=!0),t.autoplay&&(this.media.autoplay=!0),t.playbackRate!=null&&this.onceMediaEvent("canplay",()=>{t.playbackRate!=null&&(this.media.playbackRate=t.playbackRate)})}onMediaEvent(t,e,i){return this.media.addEventListener(t,e,i),()=>this.media.removeEventListener(t,e)}onceMediaEvent(t,e){return this.onMediaEvent(t,e,{once:!0})}getSrc(){return this.media.currentSrc||this.media.src||""}revokeSrc(){const t=this.getSrc();t.startsWith("blob:")&&URL.revokeObjectURL(t)}setSrc(t,e){if(this.getSrc()===t)return;this.revokeSrc();const r=e instanceof Blob?URL.createObjectURL(e):t;this.media.src=r,this.media.load()}destroy(){this.media.pause(),!this.isExternalMedia&&(this.media.remove(),this.revokeSrc(),this.media.src="",this.media.load())}setMediaElement(t){this.media=t}play(){return this.media.play()}pause(){this.media.pause()}isPlaying(){return!this.media.paused&&!this.media.ended}setTime(t){this.media.currentTime=t}getDuration(){return this.media.duration}getCurrentTime(){return this.media.currentTime}getVolume(){return this.media.volume}setVolume(t){this.media.volume=t}getMuted(){return this.media.muted}setMuted(t){this.media.muted=t}getPlaybackRate(){return this.media.playbackRate}setPlaybackRate(t,e){e!=null&&(this.media.preservesPitch=e),this.media.playbackRate=t}getMediaElement(){return this.media}setSinkId(t){return this.media.setSinkId(t)}}function it(h,t,e,i,r=5){let n=()=>{};if(!h)return n;const o=s=>{if(s.button===2)return;s.preventDefault(),s.stopPropagation(),h.style.touchAction="none";let c=s.clientX,l=s.clientY,a=!1;const d=u=>{u.preventDefault(),u.stopPropagation();const m=u.clientX,y=u.clientY;if(a||Math.abs(m-c)>=r||Math.abs(y-l)>=r){const{left:w,top:b}=h.getBoundingClientRect();a||(a=!0,e==null||e(c-w,l-b)),t(m-c,y-l,m-w,y-b),c=m,l=y}},p=u=>{a&&(u.preventDefault(),u.stopPropagation())},f=()=>{h.style.touchAction="",a&&(i==null||i()),n()};document.addEventListener("pointermove",d),document.addEventListener("pointerup",f),document.addEventListener("pointerleave",f),document.addEventListener("click",p,!0),n=()=>{document.removeEventListener("pointermove",d),document.removeEventListener("pointerup",f),document.removeEventListener("pointerleave",f),setTimeout(()=>{document.removeEventListener("click",p,!0)},10)}};return h.addEventListener("pointerdown",o),()=>{n(),h.removeEventListener("pointerdown",o)}}class P extends L{constructor(t,e){super(),this.timeouts=[],this.isScrolling=!1,this.audioData=null,this.resizeObserver=null,this.isDragging=!1,this.options=t;const i=this.parentFromOptionsContainer(t.container);this.parent=i;const[r,n]=this.initHtml();i.appendChild(r),this.container=r,this.scrollContainer=n.querySelector(".scroll"),this.wrapper=n.querySelector(".wrapper"),this.canvasWrapper=n.querySelector(".canvases"),this.progressWrapper=n.querySelector(".progress"),this.cursor=n.querySelector(".cursor"),e&&n.appendChild(e),this.initEvents()}parentFromOptionsContainer(t){let e;if(typeof t=="string"?e=document.querySelector(t):t instanceof HTMLElement&&(e=t),!e)throw new Error("Container not found");return e}initEvents(){const t=i=>{const r=this.wrapper.getBoundingClientRect(),n=i.clientX-r.left,o=i.clientX-r.left,s=n/r.width,c=o/r.height;return[s,c]};this.wrapper.addEventListener("click",i=>{const[r,n]=t(i);this.emit("click",r,n)}),this.wrapper.addEventListener("dblclick",i=>{const[r,n]=t(i);this.emit("dblclick",r,n)}),this.options.dragToSeek&&this.initDrag(),this.scrollContainer.addEventListener("scroll",()=>{const{scrollLeft:i,scrollWidth:r,clientWidth:n}=this.scrollContainer,o=i/r,s=(i+n)/r;this.emit("scroll",o,s)});const e=this.createDelay(100);this.resizeObserver=new ResizeObserver(()=>{e(()=>this.reRender())}),this.resizeObserver.observe(this.scrollContainer)}initDrag(){it(this.wrapper,(t,e,i)=>{this.emit("drag",Math.max(0,Math.min(1,i/this.wrapper.getBoundingClientRect().width)))},()=>this.isDragging=!0,()=>this.isDragging=!1)}getHeight(){return this.options.height==null?128:isNaN(Number(this.options.height))?this.options.height==="auto"&&this.parent.clientHeight||128:Number(this.options.height)}initHtml(){const t=document.createElement("div"),e=t.attachShadow({mode:"open"});return e.innerHTML=`
      <style>
        :host {
          user-select: none;
          min-width: 1px;
        }
        :host audio {
          display: block;
          width: 100%;
        }
        :host .scroll {
          overflow-x: auto;
          overflow-y: hidden;
          width: 100%;
          position: relative;
        }
        :host .noScrollbar {
          scrollbar-color: transparent;
          scrollbar-width: none;
        }
        :host .noScrollbar::-webkit-scrollbar {
          display: none;
          -webkit-appearance: none;
        }
        :host .wrapper {
          position: relative;
          overflow: visible;
          z-index: 2;
        }
        :host .canvases {
          min-height: ${this.getHeight()}px;
        }
        :host .canvases > div {
          position: relative;
        }
        :host canvas {
          display: block;
          position: absolute;
          top: 0;
          image-rendering: pixelated;
        }
        :host .progress {
          pointer-events: none;
          position: absolute;
          z-index: 2;
          top: 0;
          left: 0;
          width: 0;
          height: 100%;
          overflow: hidden;
        }
        :host .progress > div {
          position: relative;
        }
        :host .cursor {
          pointer-events: none;
          position: absolute;
          z-index: 5;
          top: 0;
          left: 0;
          height: 100%;
          border-radius: 2px;
        }
      </style>

      <div class="scroll" part="scroll">
        <div class="wrapper" part="wrapper">
          <div class="canvases"></div>
          <div class="progress" part="progress"></div>
          <div class="cursor" part="cursor"></div>
        </div>
      </div>
    `,[t,e]}setOptions(t){if(this.options.container!==t.container){const e=this.parentFromOptionsContainer(t.container);e.appendChild(this.container),this.parent=e}t.dragToSeek&&!this.options.dragToSeek&&this.initDrag(),this.options=t,this.reRender()}getWrapper(){return this.wrapper}getScroll(){return this.scrollContainer.scrollLeft}destroy(){var t;this.container.remove(),(t=this.resizeObserver)===null||t===void 0||t.disconnect()}createDelay(t=10){const e={};return this.timeouts.push(e),i=>{e.timeout&&clearTimeout(e.timeout),e.timeout=setTimeout(i,t)}}convertColorValues(t){if(!Array.isArray(t))return t||"";if(t.length<2)return t[0]||"";const e=document.createElement("canvas"),r=e.getContext("2d").createLinearGradient(0,0,0,e.height),n=1/(t.length-1);return t.forEach((o,s)=>{const c=s*n;r.addColorStop(c,o)}),r}renderBarWaveform(t,e,i,r){const n=t[0],o=t[1]||t[0],s=n.length,{width:c,height:l}=i.canvas,a=l/2,d=window.devicePixelRatio||1,p=e.barWidth?e.barWidth*d:1,f=e.barGap?e.barGap*d:e.barWidth?p/2:0,u=e.barRadius||0,m=c/(p+f)/s,y=u&&"roundRect"in i?"roundRect":"rect";i.beginPath();let w=0,b=0,S=0;for(let C=0;C<=s;C++){const g=Math.round(C*m);if(g>w){const A=Math.round(b*a*r),z=Math.round(S*a*r),B=A+z||1;let R=a-A;e.barAlign==="top"?R=0:e.barAlign==="bottom"&&(R=l-B),i[y](w*(p+f),R,p,B,u),w=g,b=0,S=0}const v=Math.abs(n[C]||0),E=Math.abs(o[C]||0);v>b&&(b=v),E>S&&(S=E)}i.fill(),i.closePath()}renderLineWaveform(t,e,i,r){const n=o=>{const s=t[o]||t[0],c=s.length,{height:l}=i.canvas,a=l/2,d=i.canvas.width/c;i.moveTo(0,a);let p=0,f=0;for(let u=0;u<=c;u++){const m=Math.round(u*d);if(m>p){const w=Math.round(f*a*r)||1,b=a+w*(o===0?-1:1);i.lineTo(p,b),p=m,f=0}const y=Math.abs(s[u]||0);y>f&&(f=y)}i.lineTo(p,a)};i.beginPath(),n(0),n(1),i.fill(),i.closePath()}renderWaveform(t,e,i){if(i.fillStyle=this.convertColorValues(e.waveColor),e.renderFunction){e.renderFunction(t,i);return}let r=e.barHeight||1;if(e.normalize){const n=Array.from(t[0]).reduce((o,s)=>Math.max(o,Math.abs(s)),0);r=n?1/n:1}if(e.barWidth||e.barGap||e.barAlign){this.renderBarWaveform(t,e,i,r);return}this.renderLineWaveform(t,e,i,r)}renderSingleCanvas(t,e,i,r,n,o,s,c){const l=window.devicePixelRatio||1,a=document.createElement("canvas"),d=t[0].length;a.width=Math.round(i*(o-n)/d),a.height=r*l,a.style.width=`${Math.floor(a.width/l)}px`,a.style.height=`${r}px`,a.style.left=`${Math.floor(n*i/l/d)}px`,s.appendChild(a);const p=a.getContext("2d");if(this.renderWaveform(t.map(f=>f.slice(n,o)),e,p),a.width>0&&a.height>0){const f=a.cloneNode(),u=f.getContext("2d");u.drawImage(a,0,0),u.globalCompositeOperation="source-in",u.fillStyle=this.convertColorValues(e.progressColor),u.fillRect(0,0,a.width,a.height),c.appendChild(f)}}renderChannel(t,e,i){const r=document.createElement("div"),n=this.getHeight();r.style.height=`${n}px`,this.canvasWrapper.style.minHeight=`${n}px`,this.canvasWrapper.appendChild(r);const o=r.cloneNode();this.progressWrapper.appendChild(o);const{scrollLeft:s,scrollWidth:c,clientWidth:l}=this.scrollContainer,a=t[0].length,d=a/c;let p=Math.min(P.MAX_CANVAS_WIDTH,l);if(e.barWidth||e.barGap){const g=e.barWidth||.5,v=e.barGap||g/2,E=g+v;p%E!==0&&(p=Math.floor(p/E)*E)}const f=Math.floor(Math.abs(s)*d),u=Math.floor(f+p*d),m=u-f,y=(g,v)=>{this.renderSingleCanvas(t,e,i,n,Math.max(0,g),Math.min(v,a),r,o)},w=this.createDelay(),b=this.createDelay(),S=(g,v)=>{y(g,v),g>0&&w(()=>{S(g-m,v-m)})},C=(g,v)=>{y(g,v),v<a&&b(()=>{C(g+m,v+m)})};S(f,u),u<a&&C(u,u+m)}render(t){this.timeouts.forEach(s=>s.timeout&&clearTimeout(s.timeout)),this.timeouts=[],this.canvasWrapper.innerHTML="",this.progressWrapper.innerHTML="",this.wrapper.style.width="",this.options.width!=null&&(this.scrollContainer.style.width=typeof this.options.width=="number"?`${this.options.width}px`:this.options.width);const e=window.devicePixelRatio||1,i=this.scrollContainer.clientWidth,r=Math.ceil(t.duration*(this.options.minPxPerSec||0));this.isScrolling=r>i;const n=this.options.fillParent&&!this.isScrolling,o=(n?i:r)*e;if(this.wrapper.style.width=n?"100%":`${r}px`,this.scrollContainer.style.overflowX=this.isScrolling?"auto":"hidden",this.scrollContainer.classList.toggle("noScrollbar",!!this.options.hideScrollbar),this.cursor.style.backgroundColor=`${this.options.cursorColor||this.options.progressColor}`,this.cursor.style.width=`${this.options.cursorWidth}px`,this.options.splitChannels)for(let s=0;s<t.numberOfChannels;s++){const c=Object.assign(Object.assign({},this.options),this.options.splitChannels[s]);this.renderChannel([t.getChannelData(s)],c,o)}else{const s=[t.getChannelData(0)];t.numberOfChannels>1&&s.push(t.getChannelData(1)),this.renderChannel(s,this.options,o)}this.audioData=t,this.emit("render")}reRender(){if(!this.audioData)return;const t=this.progressWrapper.clientWidth;this.render(this.audioData);const e=this.progressWrapper.clientWidth;this.scrollContainer.scrollLeft+=e-t}zoom(t){this.options.minPxPerSec=t,this.reRender()}scrollIntoView(t,e=!1){const{clientWidth:i,scrollLeft:r,scrollWidth:n}=this.scrollContainer,o=n*t,s=i/2,c=e&&this.options.autoCenter&&!this.isDragging?s:i;if(o>r+c||o<r)if(this.options.autoCenter&&!this.isDragging){const l=s/20;o-(r+s)>=l&&o<r+i?this.scrollContainer.scrollLeft+=l:this.scrollContainer.scrollLeft=o-s}else this.isDragging?this.scrollContainer.scrollLeft=o<r?o-10:o-i+10:this.scrollContainer.scrollLeft=o;{const{scrollLeft:l}=this.scrollContainer,a=l/n,d=(l+i)/n;this.emit("scroll",a,d)}}renderProgress(t,e){if(isNaN(t))return;const i=t*100;this.canvasWrapper.style.clipPath=`polygon(${i}% 0, 100% 0, 100% 100%, ${i}% 100%)`,this.progressWrapper.style.width=`${i}%`,this.cursor.style.left=`${i}%`,this.cursor.style.marginLeft=Math.round(i)===100?`-${this.options.cursorWidth}px`:"",this.isScrolling&&this.options.autoScroll&&this.scrollIntoView(t,e)}}P.MAX_CANVAS_WIDTH=4e3;class st extends L{constructor(){super(...arguments),this.unsubscribe=()=>{}}start(){this.unsubscribe=this.on("tick",()=>{requestAnimationFrame(()=>{this.emit("tick")})}),this.emit("tick")}stop(){this.unsubscribe()}destroy(){this.unsubscribe()}}var N=globalThis&&globalThis.__awaiter||function(h,t,e,i){function r(n){return n instanceof e?n:new e(function(o){o(n)})}return new(e||(e=Promise))(function(n,o){function s(a){try{l(i.next(a))}catch(d){o(d)}}function c(a){try{l(i.throw(a))}catch(d){o(d)}}function l(a){a.done?n(a.value):r(a.value).then(s,c)}l((i=i.apply(h,t||[])).next())})};class nt extends L{constructor(t=new AudioContext){super(),this.bufferNode=null,this.autoplay=!1,this.playStartTime=0,this.playedDuration=0,this._muted=!1,this.buffer=null,this.currentSrc="",this.paused=!0,this.crossOrigin=null,this.audioContext=t,this.gainNode=this.audioContext.createGain(),this.gainNode.connect(this.audioContext.destination)}load(){return N(this,void 0,void 0,function*(){})}get src(){return this.currentSrc}set src(t){this.currentSrc=t,fetch(t).then(e=>e.arrayBuffer()).then(e=>this.audioContext.decodeAudioData(e)).then(e=>{this.buffer=e,this.emit("loadedmetadata"),this.emit("canplay"),this.autoplay&&this.play()})}_play(){var t;this.paused&&(this.paused=!1,(t=this.bufferNode)===null||t===void 0||t.disconnect(),this.bufferNode=this.audioContext.createBufferSource(),this.bufferNode.buffer=this.buffer,this.bufferNode.connect(this.gainNode),this.playedDuration>=this.duration&&(this.playedDuration=0),this.bufferNode.start(this.audioContext.currentTime,this.playedDuration),this.playStartTime=this.audioContext.currentTime,this.bufferNode.onended=()=>{this.currentTime>=this.duration&&(this.pause(),this.emit("ended"))})}_pause(){var t;this.paused||(this.paused=!0,(t=this.bufferNode)===null||t===void 0||t.stop(),this.playedDuration+=this.audioContext.currentTime-this.playStartTime)}play(){return N(this,void 0,void 0,function*(){this._play(),this.emit("play")})}pause(){this._pause(),this.emit("pause")}setSinkId(t){return N(this,void 0,void 0,function*(){return this.audioContext.setSinkId(t)})}get playbackRate(){var t,e;return(e=(t=this.bufferNode)===null||t===void 0?void 0:t.playbackRate.value)!==null&&e!==void 0?e:1}set playbackRate(t){this.bufferNode&&(this.bufferNode.playbackRate.value=t)}get currentTime(){return this.paused?this.playedDuration:this.playedDuration+this.audioContext.currentTime-this.playStartTime}set currentTime(t){this.emit("seeking"),this.paused?this.playedDuration=t:(this._pause(),this.playedDuration=t,this._play()),this.emit("timeupdate")}get duration(){var t;return((t=this.buffer)===null||t===void 0?void 0:t.duration)||0}get volume(){return this.gainNode.gain.value}set volume(t){this.gainNode.gain.value=t,this.emit("volumechange")}get muted(){return this._muted}set muted(t){this._muted!==t&&(this._muted=t,this._muted?this.gainNode.disconnect():this.gainNode.connect(this.audioContext.destination))}getGainNode(){return this.gainNode}}var _=globalThis&&globalThis.__awaiter||function(h,t,e,i){function r(n){return n instanceof e?n:new e(function(o){o(n)})}return new(e||(e=Promise))(function(n,o){function s(a){try{l(i.next(a))}catch(d){o(d)}}function c(a){try{l(i.throw(a))}catch(d){o(d)}}function l(a){a.done?n(a.value):r(a.value).then(s,c)}l((i=i.apply(h,t||[])).next())})};const rt={waveColor:"#999",progressColor:"#555",cursorWidth:1,minPxPerSec:0,fillParent:!0,interact:!0,dragToSeek:!1,autoScroll:!0,autoCenter:!0,sampleRate:8e3};class H extends et{static create(t){return new H(t)}constructor(t){const e=t.media||(t.backend==="WebAudio"?new nt:void 0);super({media:e,mediaControls:t.mediaControls,autoplay:t.autoplay,playbackRate:t.audioRate}),this.plugins=[],this.decodedData=null,this.subscriptions=[],this.mediaSubscriptions=[],this.options=Object.assign({},rt,t),this.timer=new st;const i=e?void 0:this.getMediaElement();this.renderer=new P(this.options,i),this.initPlayerEvents(),this.initRendererEvents(),this.initTimerEvents(),this.initPlugins();const r=this.options.url||this.getSrc()||"";(r||this.options.peaks&&this.options.duration)&&this.load(r,this.options.peaks,this.options.duration)}initTimerEvents(){this.subscriptions.push(this.timer.on("tick",()=>{const t=this.getCurrentTime();this.renderer.renderProgress(t/this.getDuration(),!0),this.emit("timeupdate",t),this.emit("audioprocess",t)}))}initPlayerEvents(){this.mediaSubscriptions.push(this.onMediaEvent("timeupdate",()=>{const t=this.getCurrentTime();this.renderer.renderProgress(t/this.getDuration(),this.isPlaying()),this.emit("timeupdate",t)}),this.onMediaEvent("play",()=>{this.emit("play"),this.timer.start()}),this.onMediaEvent("pause",()=>{this.emit("pause"),this.timer.stop()}),this.onMediaEvent("emptied",()=>{this.timer.stop()}),this.onMediaEvent("ended",()=>{this.emit("finish")}),this.onMediaEvent("seeking",()=>{this.emit("seeking",this.getCurrentTime())}))}initRendererEvents(){this.subscriptions.push(this.renderer.on("click",(t,e)=>{this.options.interact&&(this.seekTo(t),this.emit("interaction",t*this.getDuration()),this.emit("click",t,e))}),this.renderer.on("dblclick",(t,e)=>{this.emit("dblclick",t,e)}),this.renderer.on("scroll",(t,e)=>{const i=this.getDuration();this.emit("scroll",t*i,e*i)}),this.renderer.on("render",()=>{this.emit("redraw")}));{let t;this.subscriptions.push(this.renderer.on("drag",e=>{this.options.interact&&(this.renderer.renderProgress(e),clearTimeout(t),t=setTimeout(()=>{this.seekTo(e)},this.isPlaying()?0:200),this.emit("interaction",e*this.getDuration()),this.emit("drag",e))}))}}initPlugins(){var t;!((t=this.options.plugins)===null||t===void 0)&&t.length&&this.options.plugins.forEach(e=>{this.registerPlugin(e)})}unsubscribePlayerEvents(){this.mediaSubscriptions.forEach(t=>t()),this.mediaSubscriptions=[]}setOptions(t){this.options=Object.assign({},this.options,t),this.renderer.setOptions(this.options),t.audioRate&&this.setPlaybackRate(t.audioRate),t.mediaControls!=null&&(this.getMediaElement().controls=t.mediaControls)}registerPlugin(t){return t.init(this),this.plugins.push(t),this.subscriptions.push(t.once("destroy",()=>{this.plugins=this.plugins.filter(e=>e!==t)})),t}getWrapper(){return this.renderer.getWrapper()}getScroll(){return this.renderer.getScroll()}getActivePlugins(){return this.plugins}loadAudio(t,e,i,r){return _(this,void 0,void 0,function*(){if(this.emit("load",t),!this.options.media&&this.isPlaying()&&this.pause(),this.decodedData=null,!e&&!i){const o=s=>this.emit("loading",s);e=yield tt.fetchBlob(t,o,this.options.fetchParams)}this.setSrc(t,e);const n=(yield Promise.resolve(r||this.getDuration()))||(yield new Promise(o=>{this.onceMediaEvent("loadedmetadata",()=>o(this.getDuration()))}));if(i)this.decodedData=$.createBuffer(i,n||0);else if(e){const o=yield e.arrayBuffer();this.decodedData=yield $.decode(o,this.options.sampleRate)}this.decodedData&&(this.emit("decode",this.getDuration()),this.renderer.render(this.decodedData)),this.emit("ready",this.getDuration())})}load(t,e,i){return _(this,void 0,void 0,function*(){yield this.loadAudio(t,void 0,e,i)})}loadBlob(t,e,i){return _(this,void 0,void 0,function*(){yield this.loadAudio("blob",t,e,i)})}zoom(t){if(!this.decodedData)throw new Error("No audio loaded");this.renderer.zoom(t),this.emit("zoom",t)}getDecodedData(){return this.decodedData}exportPeaks({channels:t=2,maxLength:e=8e3,precision:i=1e4}={}){if(!this.decodedData)throw new Error("The audio has not been decoded yet");const r=Math.min(t,this.decodedData.numberOfChannels),n=[];for(let o=0;o<r;o++){const s=this.decodedData.getChannelData(o),c=[],l=Math.round(s.length/e);for(let a=0;a<e;a++){const d=s.slice(a*l,(a+1)*l),p=Math.max(...d);c.push(Math.round(p*i)/i)}n.push(c)}return n}getDuration(){let t=super.getDuration()||0;return(t===0||t===1/0)&&this.decodedData&&(t=this.decodedData.duration),t}toggleInteraction(t){this.options.interact=t}seekTo(t){const e=this.getDuration()*t;this.setTime(e)}playPause(){return _(this,void 0,void 0,function*(){return this.isPlaying()?this.pause():this.play()})}stop(){this.pause(),this.setTime(0)}skip(t){this.setTime(this.getCurrentTime()+t)}empty(){this.load("",[[0]],.001)}setMediaElement(t){this.unsubscribePlayerEvents(),super.setMediaElement(t),this.initPlayerEvents()}destroy(){this.emit("destroy"),this.plugins.forEach(t=>t.destroy()),this.subscriptions.forEach(t=>t()),this.unsubscribePlayerEvents(),this.timer.destroy(),this.renderer.destroy(),super.destroy()}}const ot=h=>{const t=atob(h),e=Array.from(t).map(i=>i.charCodeAt(0));return new Uint8Array(e)},gt=h=>{h.getTracks().forEach(t=>t.stop())},at=h=>{try{return JSON.parse(h)}catch{return h}},ht=(h="/voice")=>{const t=window.location.protocol==="https:"?"wss:":"ws:",e=window.location.host;return new URL(h,`${t}//${e}`).href},vt=h=>{const t=x(""),e=x([]),i=x([]);return{currentText:t,originals:e,translateds:i,onmessage:o=>{var c,l;const s=at(o);if(typeof s!="string"){if(s.type==="original"&&s.content)s.isFinal?(t.value="",e.value.push(s.content)):t.value=s.content;else if(s.type==="translated")i.value.push(s.content),(c=h==null?void 0:h.ontranslated)==null||c.call(h,s.content);else if(s.type==="audio"&&s.content){const a=ot(s.content),d=new Blob([a],{type:"audio/mp3"});(l=h==null?void 0:h.onAudioData)==null||l.call(h,d)}}},cleanScreen:()=>{t.value="",e.value=[],i.value=[]}}},yt=(h,t)=>{const e=new WebSocket(ht(h)),i=[];return e.onopen=()=>{var o;i.length>0&&(i.forEach(s=>{typeof s=="string"||s instanceof Blob||s instanceof ArrayBuffer?e.send(s):e.send(JSON.stringify(s))}),i.length=0),(o=t.onopen)==null||o.call(t,e)},e.onmessage=o=>{var s;return(s=t.onmessage)==null?void 0:s.call(t,o.data)},e.onerror=o=>{var s;(s=t.onerror)==null||s.call(t,o)},e.onclose=o=>{var s;(s=t.onclose)==null||s.call(t,o)},{ws:e,send:o=>{e.readyState===WebSocket.OPEN?typeof o=="string"||o instanceof Blob||o instanceof ArrayBuffer?e.send(o):e.send(JSON.stringify(o)):i.push(o)},close:()=>{e.close()}}},lt=V({__name:"ScrollableContent",props:{isLoading:{type:Boolean,default:!1},contents:{type:Array,default:()=>[]},current:{type:String}},setup(h){const t=x();return X(t,e=>{var r;const i=(r=t.value)==null?void 0:r.lastElementChild;i&&i.scrollIntoView({behavior:"smooth",block:"end"})},{childList:!0}),(e,i)=>(M(),T("section",{class:k([e.$style["lru-wrapper"]])},[D("section",{class:k([e.$style.lru])},[D("ul",{class:k([e.$style["lru-content"]]),ref_key:"listRef",ref:t},[(M(!0),T(G,null,I(h.contents,(r,n)=>(M(),T("li",{key:r+n,class:k([e.$style["lru-item"]])},O(r),3))),128)),h.current?(M(),T("li",{key:0,class:k([e.$style["lru-item"]])},O(h.current),3)):q("",!0),D("li",{class:k([e.$style.hidden])},null,2)],2)],2)],2))}}),ct="_lru_1ctad_2",dt="_hidden_1ctad_63",ut={"lru-wrapper":"_lru-wrapper_1ctad_2",lru:ct,"lru-content":"_lru-content_1ctad_33","lru-item":"_lru-item_1ctad_43",hidden:dt},pt={$style:ut},bt=U(lt,[["__cssModules",pt]]);export{bt as S,H as W,mt as X,yt as c,gt as s,vt as u};
