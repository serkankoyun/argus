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

## Sequence
pane-sequence = Akış · en yeni altta
pane-tasks = Görevler
pane-message = Son mesaj
unassigned = atanmadı
tasks-none = Henüz görev yok (Agent Teams görev listeleri burada görünür).

## Fleet
col-region = BÖLGE
col-collector = COLLECTOR
col-rtt = GECİKME
col-cpu = CPU
col-mem = BELLEK
col-agents = AJAN
col-last = SON OLAY
collector-ok = sağlıklı
collector-down = yeniden bağlanıyor
ago = { $t } önce
pane-alerts = Uyarılar
alert-waiting = { $what }
alert-stalled = takıldı · { $what }
alert-context = context %{ $p } · yakında sıkıştırılacak
alert-host-down = collector bağlantısı koptu · yerelde biriktiriyor
alerts-none = Her şey yolunda.
pane-hub = Veri kaynağı
info-source = kaynak
info-hosts = sunucu
info-agents = ajan
info-messages = mesaj
info-cost = maliyet (tahmini)
info-connect = Makine eklemek için: : tuşuna bas, bağlan kullanıcı@sunucu yaz
cost-by-agent = { $host } · ajan bazında maliyet

## Channel
pane-channels = Kanallar
chan-approvals = onaylar
chan-direct = özel mesajlar
chan-hosts = sunucular
chan-perm = { $who } ONAY İSTİYOR → sen: { $what }   [a] onayla  [r] reddet
chan-input-hint = :mesaj <metin> · a onayla · > yeni iş
pane-members = Kişiler · şu an

## Task map
pane-taskmap = Bağımlılıklar ve teslimler
pane-bottlenecks = Darboğazlar
bottlenecks = Darboğazlar
bottlenecks-none = Engellenen görev yok.
holds-n = { $n } görevi bekletiyor
critical-path = Kritik yol
tasks-progress = { $total } görevden { $done } tanesi bitti
pane-utilization = Ajan doluluğu · 15 dk

## Pair
pair-none = Henüz konuşma partneri yok.
pair-quiet = Şu an yolda mesaj yok.
pair-history = önceki mesajlar
pair-channel = Aralarındaki kanal
pair-others = Diğerleri şu an

## Commands
key-approve = onayla
key-reject = reddet
key-cmd = komut
key-prompt = yeni iş
input-cmd-hint = :bağlan kullanıcı@sunucu · :mesaj <metin> · :çalıştır <klasör> <istem> · :görünüm <ad> · :dil en · esc
input-prompt-hint = seçili ajanın projesinde yeni bir Claude Code işi için istem · başlatmak için enter
no-approvals = bekleyen onay yok
unknown-cmd = bilinmeyen komut: { $cmd }

## Teams
view-teams = Takımlar
team-label = takım
key-team = sonraki takım
teams-agents = ajan
teams-tasks = { $n } açık görev
teams-msgs = mesaj 15dk
teams-filtered = tüm ekranlar bu takıma filtreli · değiştirmek için t
teams-cross = Takımlar arası istekler
teams-from = İSTEYEN TAKIM
teams-to = İSTENEN TAKIM
teams-count = MESAJ
teams-latest = SON
teams-open = takıma gir
key-team-move = takım seç
key-leave-team = tüm takımlar

## Hosts filter and history
host-label = sunucu
key-host = sunucu
key-show-done = bitenleri göster
key-hide-done = bitenleri gizle
fleet-hint = ↑↓ sunucu seç · enter yalnız o sunucuyu gösterir · s her ekranda sunucu değiştirir
view-history = Geçmiş
history-title = Biten ajanlar · { $n } · ${ $cost }
history-hint = biten ajanlar diğer ekranlarda gizlidir · f ile orada da gösterilir
history-empty = Henüz biten ajan yok.
history-last = Son biten
col-team = TAKIM
col-finished = BİTİŞ
col-outcome = SONUÇ
empty-hidden = { $n } oturum gizli: bitmişler ya da sunucu veya takım filtresi açık. Bitenleri göstermek için f, filtreleri kaldırmak için esc, Geçmiş için H.

## argus init
init-default-team = ekipsiz
init-none = { $n } ajan adında ekip kalıbı bulunamadı. rol-ekip-iş biçimindeki adlar (örneğin qa-odeme-iade) gruplanabilir; değilse teams.json dosyasını elle yazın (INSTALL.md).
init-preview = Bu kurallarla ekip başına ajan sayısı:
init-hint = Hiçbir şey kaydedilmedi. Kuralları { $path } dosyasına kaydetmek için `argus init --write` çalıştırın, sonra gerekirse dosyayı düzenleyin.
init-written = { $path } dosyasına kaydedildi. Argus bir saniye içinde okur; ekipleri yeniden adlandırmak veya ad eklemek için dosyayı düzenleyin.
init-missing-file = Uyarı: mevcut teams.json içindeki teams_file { $path } yok; bu yüzden oradaki ekip kodları ve adları kullanılmıyor. Var olan bir dosyayı gösterin.

## Plain-word activities
act-test = test koşuyor
act-build = derliyor
act-install = paket kuruyor
act-wait = bekliyor
act-inspect = dosyalara bakıyor
warn-teams-file = teams_file { $arg } yok: oradaki ekip kodları ve adları kullanılmıyor
teams-idle = biten ekipler
teams-coordinator = Koordinatör
