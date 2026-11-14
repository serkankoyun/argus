# Argus — Türkçe

## Views
view-overview = Genel
view-sequence = Akış
view-agent = Ajan
view-fleet = Sunucular
view-graph = Ağ
view-room = Oda
view-lanes = Şeritler
view-channel = Kanal
view-taskmap = Görev haritası
view-pair = İkili

placeholder = Bu ekran { $milestone } sürümünde geliyor. Genel görünüm için 1'e bas.

## Header
header-summary = { $hosts } sunucu · { $agents } ajan · { $active } aktif
status-live = canlı
status-paused = duraklatıldı

## Panes
pane-hosts = Sunucular
pane-agents = Ajanlar

## Agent table columns
col-agent = AJAN
col-host = SUNUCU
col-state = DURUM
col-tool = ARAÇ
col-tokens = TOKEN
col-context = CONTEXT
col-cost = MALİYET
col-time = SÜRE

## Agent states
state-working = çalışıyor
state-waiting = bekliyor
state-done = bitti
state-idle = boşta
state-stalled = takıldı

## Key hints
key-views = ekranlar
key-next = sonraki ekran
key-select = seç
key-pause = duraklat
key-quit = çık

## v0.1 additions
status-demo = demo
header-waiting = { $n } bekliyor
header-stalled = { $n } takıldı
loading = Claude Code oturumları okunuyor…
empty = { $dir } içinde yakın zamanda Claude Code oturumu yok. Bir tane başlat ya da `argus --demo` dene.
pane-flow = Akış · görevler ve mesajlar
pane-metrics = Metrikler
pane-events = Olaylar
flow-empty = Ajanlar arasında henüz mesaj yok.
m-tokens = token/sn
m-cost = maliyet/sa
m-tools = araç/dk
m-msgs = mesaj/dk
m-errors = hata
m-ctx = ort. ctx
key-detail = detay

## What an agent is doing now
now-thinking = düşünüyor…
now-waiting = onay bekliyor: { $what }
now-idle = girdi bekliyor
now-done = bitti
now-stalled = ilerleme yok ({ $what })

## Message kinds
kind-task = görev
kind-msg = mesaj
kind-result = sonuç
kind-perm = onay?
kind-finding = bulgu

## Task status
task-pending = BEKLEMEDE
task-in-progress = SÜRÜYOR
task-blocked = ENGELLİ
task-done = BİTTİ

## Graph
pane-graph = Canlı ağ
graph-legend = ● görev  ◆ mesaj  ✓ sonuç  ▲ onay  ! bulgu · parlak çizgi = yoğun trafik
pane-traffic = Trafik · 15 dk
pane-now = Şimdi
you = sen
approvals-pending = { $n } onay bekliyor
press-a = onay için a'ya bas

## Room
pane-room = Kim ne yapıyor, kimden ne istiyor
now-label = şu an
pane-requests = Açık istekler
pane-conversations = Son konuşmalar
req-answered = ✓ CEVAPLANDI
req-answered-in = { $t } içinde cevaplandı
req-approval = ONAY BEKLİYOR
req-new = YENİ
req-working = ÇALIŞIYOR
req-waiting = { $t } bekliyor
req-running = { $t } sürüyor
req-none = Açık istek yok.

## Lanes
pane-lanes = Ajanlar ve aralarındaki mesajlar
lanes-legend = Read · Edit · Bash · ajanlar · web  │  ┊ mesaj  │  z yakınlaştır ({ $w })
lanes-now = şimdi
lanes-now-col = ŞU AN / NE BEKLİYOR
lanes-waiting-for = bekliyor:
lanes-asked-by = isteyen:
pane-matrix = Kim kimden ne istiyor
matrix-corner = isteyen ↓ / istenen →

## Agent detail
agent-none = Seçili ajan yok. Genel ekranda ↑↓ ile bir ajan seç.
agent-model = model
agent-parent = üst
agent-project = proje
agent-branch = dal
agent-context = Context
agent-ctx-used = kullanılan
agent-ctx-free = boş
agent-compact = auto-compact
agent-compact-now = yakında
agent-compact-at = %{ $p }'de
agent-tokens = Token ve maliyet (tahmini)
agent-input = girdi
agent-output = çıktı
agent-cache-read = cache okuma
agent-cache-write = cache yazma
agent-cost = maliyet
agent-tools = araç çağrısı
agent-errors = hata
agent-activity = Aktivite · araç çağrısı/dk · 30 dk
agent-tool-calls = Araç çağrıları
agent-files = Değişen dosyalar
agent-no-files = Henüz dosya değiştirilmedi.
agent-transcript = Transcript
col-target = HEDEF
col-dur = SÜRE
col-status = DURUM
status-running = sürüyor
