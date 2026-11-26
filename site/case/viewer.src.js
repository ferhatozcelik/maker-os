// MakerOS kasası: kaydırdıkça parça parça birleşen 3D görünüm.
// Kaynak bu dosya; sayfa paketlenmiş viewer.js'i yükler. Yeniden üretmek için:
//   npm i three@0.187.0 esbuild && npx esbuild site/case/viewer.src.js --bundle --minify --format=esm --outfile=site/case/viewer.js
// Model (case.glb) RPi4_Kamera_Kutusu_Export içindeki STEP'ten üretildi; 3MF ile aynı ölçülerdedir (mm).
import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

const stage = document.getElementById("case-stage");
const track = document.getElementById("case-scroll");
const rail = document.getElementById("case-rail");
if (stage && track) init();

function init() {
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce) track.classList.add("still");

  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  } catch {
    stage.classList.add("no-gl");
    return;
  }
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.05;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  stage.prepend(renderer.domElement);

  const scene = new THREE.Scene();
  const pmrem = new THREE.PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
  scene.environmentIntensity = 0.55;

  const camera = new THREE.PerspectiveCamera(30, 1, 1, 5000);

  // Işıklar: sıcak ana ışık (gölge verir), soğuk kenar ışığı, yumuşak dolgu.
  const key = new THREE.DirectionalLight(0xffd2a8, 2.4);
  key.castShadow = true;
  key.shadow.mapSize.set(2048, 2048);
  key.shadow.radius = 6;
  key.shadow.bias = -0.0004;
  scene.add(key, key.target);
  const rim = new THREE.DirectionalLight(0xcfe2ff, 1.6);
  scene.add(rim);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x3a3027, 0.5));

  const ground = new THREE.Mesh(new THREE.PlaneGeometry(2000, 2000), new THREE.ShadowMaterial({ opacity: 0.12 }));
  ground.rotation.x = -Math.PI / 2;
  ground.receiveShadow = true;
  scene.add(ground);

  // Birleşme sırası: kutu → Pi 4 → kamera tutucu → kamera → ön panel (logo dahil).
  const ORDER = [
    (n) => n === "Box_Back",
    (n) => n.startsWith("REF_Pi4"),
    (n) => n === "Camera_Clamp",
    (n) => n.startsWith("REF_Cam"),
    (n) => n === "Front_Panel" || n.startsWith("Logo"),
  ];
  const groups = ORDER.map(() => new THREE.Group());
  // Sitedeki kasa renkleri: haki yeşil gövde, krem logo, kahverengi logo yuvası (3MF dosyasındaki renkler değişmez).
  const TINT = { Box_Back: "#565c3a", Front_Panel: "#565c3a", Logo_Orange: "#f1e7cf", Logo_Black: "#4a3221", Camera_Clamp: "#3b3a33" };
  const copies = [...stage.querySelectorAll("[data-beat]")];
  const spin = [0, 0.5, -0.6, 0.7, -0.4]; // gelirken hafif dönüş (rad)

  // Kaydırma ilerlemesi: [A0, A1] aralığında parçalar sırayla yerine oturur, sonrası kamerayı kapağa çevirir.
  const A0 = 0.04, A1 = 0.8, STEPS = ORDER.length - 1;
  const beats = [0, ...Array.from({ length: STEPS }, (_, i) => A0 + ((i + 1) / STEPS) * (A1 - A0))];
  beats[beats.length - 1] = 1; // son nokta: kapak görünümü
  const ease = (t) => (t <= 0 ? 0 : t >= 1 ? 1 : t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const lerp = (a, b, t) => a + (b - a) * t;

  let size = null, gap = 40, ready = false;
  const home = [];

  new GLTFLoader().load(new URL("case.glb", import.meta.url).href, (gltf) => {
    const root = gltf.scene;
    root.rotation.x = Math.PI / 2; // glTF'de -Z olan CAD Y ekseni (kutunun ağzı) yukarı baksın
    root.updateMatrixWorld(true);
    const meshes = [];
    root.traverse((o) => o.isMesh && meshes.push(o));
    const box = new THREE.Box3();
    for (const m of meshes) {
      m.geometry.applyMatrix4(m.matrixWorld);
      m.geometry.computeBoundingBox();
      box.union(m.geometry.boundingBox);
    }
    const c = box.getCenter(new THREE.Vector3());
    const shift = new THREE.Matrix4().makeTranslation(-c.x, -box.min.y, -c.z); // tabanı y=0'a, ortayı eksene al
    for (const m of meshes) {
      m.geometry.applyMatrix4(shift);
      m.geometry.computeBoundingBox();
      const g = ORDER.findIndex((f) => f(m.name));
      if (g < 0) continue;
      m.position.set(0, 0, 0); m.rotation.set(0, 0, 0); m.scale.set(1, 1, 1);
      m.castShadow = true;
      m.userData.g = g;
      const tint = TINT[m.name];
      if (tint && m.material) { m.material = m.material.clone(); m.material.color.set(tint); m.material.roughness = 0.55; m.material.metalness = 0; }
      groups[g].add(m);
    }
    size = box.getSize(new THREE.Vector3());
    gap = size.y * 1.35;
    groups.forEach((g, i) => {
      // Her grubu kendi merkezi etrafında döndürebilmek için pivotu merkeze taşı.
      const gb = new THREE.Box3().setFromObject(g);
      const gc = gb.isEmpty() ? new THREE.Vector3() : gb.getCenter(new THREE.Vector3());
      g.children.forEach((m) => m.position.sub(gc));
      g.position.copy(gc);
      home[i] = gc.clone();
      scene.add(g);
    });
    const r = Math.max(size.x, size.z);
    key.shadow.camera.left = key.shadow.camera.bottom = -r * 1.6;
    key.shadow.camera.right = key.shadow.camera.top = r * 1.6;
    key.shadow.camera.near = 1; key.shadow.camera.far = r * 12;
    key.shadow.camera.updateProjectionMatrix();
    ready = true;
    stage.classList.add("ready");
    resize(); wake();
  }, undefined, () => stage.classList.add("no-gl"));

  // Sürükleyerek döndürme (dikey kaydırma sayfada kalır).
  let dragAz = 0, dragEl = 0, velAz = 0, dragging = false, lx = 0, ly = 0;
  const cv = renderer.domElement;
  cv.addEventListener("pointerdown", (e) => { dragging = true; lx = e.clientX; ly = e.clientY; cv.setPointerCapture(e.pointerId); stage.classList.add("grab"); wake(); });
  cv.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const dx = e.clientX - lx, dy = e.clientY - ly; lx = e.clientX; ly = e.clientY;
    velAz = dx * 0.008; dragAz += velAz;
    if (e.pointerType === "mouse") dragEl = clamp(dragEl + dy * 0.005, -0.5, 0.6);
    wake();
  });
  const up = () => { dragging = false; stage.classList.remove("grab"); };
  cv.addEventListener("pointerup", up);
  cv.addEventListener("pointercancel", up);

  // Ölçek göstergesi: noktalar yalnızca ilerlemeyi gösterir, yazı yok.
  const dots = [];
  if (rail) {
    beats.forEach((b, i) => {
      const d = document.createElement("button");
      d.type = "button"; d.className = "dot";
      d.style.left = `${(i / (beats.length - 1)) * 100}%`;
      d.setAttribute("aria-label", `${i + 1} / ${beats.length}`);
      d.onclick = () => scrollToProgress(b);
      rail.append(d); dots.push(d);
    });
  }
  const fill = rail && rail.querySelector(".fill");

  function scrollRange() {
    const r = track.getBoundingClientRect();
    return { top: r.top + scrollY, len: Math.max(1, track.offsetHeight - stage.offsetHeight) };
  }
  function target() {
    if (reduce) return 1;
    const { top, len } = scrollRange();
    return clamp((scrollY - top + parseFloat(getComputedStyle(stage).top || 0)) / len, 0, 1);
  }
  function scrollToProgress(p) {
    if (reduce) return;
    const { top, len } = scrollRange();
    scrollTo({ top: top - parseFloat(getComputedStyle(stage).top || 0) + p * len, behavior: "smooth" });
  }

  function resize() {
    const w = stage.clientWidth, h = stage.clientHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    narrow = w < 800;
    if (narrow) camera.setViewOffset(w, h, 0, h * 0.27, w, h);
    else camera.setViewOffset(w, h, -w * 0.2, h * 0.07, w, h);
    camera.updateProjectionMatrix();
    wake();
  }
  new ResizeObserver(resize).observe(stage);

  // Başlıklar: en son yerine oturan parçanın metni görünür.
  let shown = -1, narrow = false;
  function setBeat(v) {
    let i = 0;
    beats.forEach((b, k) => { if (v >= (k === 0 ? 0 : (beats[k - 1] + b) / 2)) i = k; });
    if (i === shown || reduce) return;
    shown = i;
    copies.forEach((c) => c.classList.toggle("on", +c.dataset.beat === i));
  }
  if (!reduce) setBeat(target());

  let p = target(), running = false, visible = false, t0 = performance.now();
  const loop = (now) => {
    running = false;
    if (!visible) return;
    const goal = target();
    p += (goal - p) * 0.09;
    if (Math.abs(goal - p) < 1e-4) p = goal;
    if (!dragging) { velAz *= 0.93; dragAz += velAz; }
    if (ready) draw((now - t0) / 1000);
    running = true;
    requestAnimationFrame(loop);
  };
  function wake() { if (!running && visible) { running = true; requestAnimationFrame(loop); } }
  new IntersectionObserver((es) => { visible = es[0].isIntersecting; wake(); }).observe(stage);
  addEventListener("scroll", wake, { passive: true });

  function draw(time) {
    // Her grubun iniş ilerlemesi; üstteki parçalar alttakilerin boşluğunu da taşır, yığın sıkışarak kapanır.
    const a = clamp((p - A0) / (A1 - A0), 0, 1) * STEPS;
    let lift = 0, top = size.y;
    for (let i = 0; i < groups.length; i++) {
      const t = i === 0 ? 1 : ease(clamp(a - (i - 1), 0, 1));
      if (i > 0) lift += gap * (1 - t);
      const g = groups[i];
      g.position.set(home[i].x, home[i].y + lift, home[i].z);
      g.rotation.y = spin[i] * (1 - t);
      top = Math.max(top, size.y + lift);
    }
    dots.forEach((d, i) => { d.classList.toggle("on", p >= beats[i] - 0.004); });
    setBeat(p);
    if (fill) fill.style.width = `${p * 100}%`;

    // Kamera: yan profilden başlar, birleştikçe yaklaşır ve kapağa (logo) doğru yükselir.
    const end = ease(clamp((p - 0.62) / 0.38, 0, 1));
    const az = lerp(-0.85, 0.55, ease(p)) + Math.sin(time * 0.35) * 0.04 + dragAz;
    const el = clamp(lerp(0.32, 0.82, end) + dragEl, 0.05, 1.35);
    const cy = top * 0.5;
    const rad = Math.hypot(size.x, size.z, top) * 0.5;
    const fit = narrow ? Math.max(1, 1.5 / camera.aspect) : Math.max(1, 1.9 / camera.aspect);
    const dist = (rad / Math.sin(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.38 * Math.min(fit, 2.4);
    camera.position.set(Math.sin(az) * Math.cos(el) * dist, cy + Math.sin(el) * dist, Math.cos(az) * Math.cos(el) * dist);
    camera.lookAt(0, cy, 0);

    const L = Math.max(size.x, size.z) * 4;
    key.position.set(L * 0.6, L * 1.2 + top, L * 0.5);
    key.target.position.set(0, top * 0.3, 0);
    rim.position.set(-L, L * 0.5, -L * 0.8);
    renderer.render(scene, camera);
  }
}
