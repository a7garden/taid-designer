# Taid

게임 PNG를 프로젝트에 보관하고 로컬 규격 검사, 선택적 Codex 검토, 픽셀 정리와 전달을 수행하는 Tauri 데스크톱 앱입니다. GUI와 `taid` CLI는 같은 엔진과 승인 조건을 사용합니다.

## 작업 순서

1. **프로젝트를 만들거나 엽니다.** PNG 파일을 놓기, 파일 선택, 클립보드 붙여넣기로 추가합니다. 프로젝트가 없으면 저장 위치를 먼저 정합니다. 원본은 프로젝트에 복사해 보존합니다.
2. **로컬 검사 결과를 봅니다.** 가져오기는 크기·알파·도트 격자·팔레트·시퀀스 등을 검사하며 AI를 호출하지 않습니다. `추가 후 AI 검토`는 기본 꺼짐입니다. 직접 AI 검토를 실행하거나 이 옵션을 켠 경우에만 이미지와 검토 기준을 Codex에 보냅니다.
3. **제작 기준을 맞춥니다.** 상단 `제작 기준`에서 도트 크기, 물리 PNG 너비·높이(px), 알파 정책, 승인 팔레트와 메모를 저장합니다. 저장하면 연결 에셋을 재검사하고 기존 승인이 무효화될 수 있습니다. `generic`은 가져온 캔버스를 유지하며 Stage 03 전용 규칙을 강제하지 않습니다.
4. **필요한 처리만 실행합니다.** 누끼, 격자 정리, 팔레트 매핑, 타일 배치를 각각 선택합니다. 결과를 확대·실제 크기·프레임 재생으로 확인하세요. 검사 통과는 그림의 품질 승인과 다릅니다.
5. **디자인과 QC를 각각 승인한 뒤 내보냅니다.** 두 승인이 현재 상태에 유효하고 QC 오류가 없으며 참고용이 아닌 에셋만 전달할 수 있습니다. 내보내기는 새 전달 폴더를 만들고 PNG, 매니페스트, CSV와 전달 메모를 기록합니다. 원본 파일을 덮어쓰지 않습니다.

### 클립보드와 프로젝트 전환

`클립보드 붙여넣기` 버튼 또는 입력란 밖에서 Ctrl+V / Cmd+V로 이미지를 추가합니다. 텍스트 입력란·텍스트 편집 영역, IME 조합 중, 열린 대화상자에서는 전역 이미지 붙여넣기를 가로채지 않습니다. 이름·메모·팔레트 입력에는 평소처럼 텍스트를 붙여넣을 수 있습니다.

프로젝트 메뉴에는 최근 20개 선택이 남습니다. 이동되거나 연결이 끊긴 경로도 목록에서 사라지지 않습니다. 올바른 위치를 `프로젝트 열기`로 다시 선택하세요. CLI나 다른 창이 프로젝트를 바꾸면 `외부에서 변경됨`의 `다시 불러오기`를 사용합니다. 저장 전 편집값은 전환 시 확인하며, 오래된 상태에서 쓰는 작업은 거부됩니다. 오류를 무시해 재시도하지 말고 새 상태를 읽으세요.

## 누끼·팔레트·타일

### 로컬 자동 누끼

`자동 누끼`는 U2Netp 신경망을 로컬 CPU에서 실행합니다. 최초 명시적 실행 때만 4.4 MiB(4,574,861바이트) ONNX 가중치를 다운로드하고 크기와 SHA-256을 확인해 캐시합니다. 모델은 **Apache-2.0**이며 rembg 소스의 MIT 라이선스와 구분해야 합니다. 이미지 자체는 다운로드 서버나 Codex에 보내지 않습니다. 캐시가 준비되면 누끼 추론에 네트워크가 필요하지 않습니다.

현재 이미지에서 알파 마스크를 만들고 RGB와 캔버스 크기는 유지합니다. 원래 투명한 부분은 다시 불투명하게 만들지 않습니다. 원본·부모 결과를 보존하는 새 미승인 변형을 추가합니다. 320×320 입력으로 추론하는 주요 피사체 분리 모델이므로 가는 선, 머리카락, 반투명 이펙트, 작은 도트와 복잡한 배경은 직접 확인해야 합니다. 픽셀아트 전용 분할이나 수작업 마스크를 대신하지 않으며, 경계가 도트·알파 규칙에 맞지 않으면 결과를 검토한 뒤 별도로 정리합니다.

### 실제 색상 검사와 승인 팔레트

팔레트 검사는 **현재 파일**의 보이는 RGB 색 수와 빈도, 승인 팔레트 밖 색을 읽습니다. 완전 투명 픽셀은 제외하고 반투명 픽셀은 RGB별로 셉니다. 승인 팔레트가 없으면 적합 여부를 판정하지 않습니다.

`.hex` / GIMP `.gpl` 파일을 읽거나 `#RRGGBB`를 입력해 최대 256색을 제작 기준에 저장합니다. GUI의 파일 읽기는 초안을 채우는 동작이며 `이 제작 기준에 팔레트 저장`을 눌러야 적용됩니다. 현재 이미지가 256색 이하이면 실제 색을 초안으로 가져올 수도 있지만, 그것이 게임의 승인 팔레트라는 뜻은 아닙니다. 저장은 기준을 바꾸며 아직 픽셀 색을 바꾸지 않습니다.

`승인 팔레트로 색상 변환`은 저장된 각 에셋의 팔레트에 RGB 제곱 거리로 매핑합니다. 알파는 유지하고 새 미승인 변형을 만듭니다. 색상 의미·명암·실루엣을 이해하는 예술적 수정은 아닙니다. 같은 색 수라도 다른 팔레트일 수 있으므로 실제 게임 팔레트를 사용하세요.

### 논리 타일과 물리 PNG

타일 너비·높이는 **논리 도트 캔버스**, 배율은 **px/도트**입니다. `32×32도트 × 4px/도트 = 128×128px PNG`입니다. 제작 기준의 너비·높이(px)와 혼동하지 마세요. 입력은 현재 제작 기준의 도트 크기로 나누어 떨어지고 각 도트 블록이 균일해야 합니다. 맞지 않으면 도트 크기를 확인하고 격자 정리를 먼저 선택합니다.

- `pad`: 그림 전체 캔버스를 유지하고 투명 여백을 추가합니다. 큰 입력을 몰래 자르지 않고 거부합니다.
- `fit`: 작은 입력은 들어가는 최대 정수 배율로 확대하고 큰 입력은 최근접 도트로 축소합니다. 축소하면 한 도트 세부나 윤곽이 사라질 수 있습니다.
- 정렬은 `center` 또는 `bottom-center`이며 보이는 피사체만 자동 재중앙화하지 않습니다. 피벗은 배치·크기 변화에 맞춰 변환합니다.

타일은 현재 파일에서 새 변형과 전용 제작 기준을 만듭니다. 부모 파일과 기준은 유지하며 팔레트·알파 정책, 프레임 시간, 레이어와 참고용 상태를 이어받습니다. 누끼 → 팔레트 → 타일처럼 이어서 작업할 때는 **새 결과의 ID**를 다음 작업에 사용하세요. 반면 `process`의 격자·알파·팔레트·트림·확대는 항상 해당 에셋의 보존 원본에서 다시 시작합니다.

## Codex 설정과 범위

Codex CLI를 별도로 설치하고 본인 계정으로 로그인해야 합니다. `codex --version`, `codex login status`로 설치·로그인 상태를 확인할 수 있습니다. Taid는 설치된 CLI의 `app-server`를 사용합니다. 상단 `모델 설정`에서 CLI가 제공하는 모델 목록과 지원 추론 강도를 확인하고 저장하면 다음 검토에 적용됩니다. 비어 있는 모델 ID는 Codex 기본 모델을 사용합니다. 설정 저장만으로 AI 검토를 실행하지 않습니다.

검토는 선택 이미지와 요청문·기준을 전송하며 **로그인한 Codex 계정의 사용량/한도**를 소비합니다. ChatGPT 웹 채팅 할당량을 우회하거나 숨겨진 무료 API를 쓰는 방식이 아닙니다. 실제 사용 조건은 계정·플랜·인증 방식에 따릅니다. 회사 자료를 보내기 전에 조직 정책과 권한을 확인하세요.

AI 검토는 조언만 제공합니다. 분석 연결에서는 셸·웹·파일 변경·이미지 생성 도구를 끄고 읽기 전용 요청을 사용하며 승인/권한 요청을 자동 수락하지 않습니다. 이미지 생성, 그림의 자동 예술적 수정, Unity atlas 패킹과 `.meta`/Importer 편집은 제공하지 않습니다. 기존 ChatGPT 작업에 사용할 제작 기준 프롬프트를 복사하는 기능은 있지만, 복사 자체가 이미지를 전송하지는 않습니다.

## CLI / Codex에서 조작

설치된 CLI가 PATH에 없으면 실행 파일의 전체 경로를 쓰세요. 개발 중에는 `npm run cli -- --help`로 같은 CLI를 실행합니다. 아래 변수는 실제 경로·반환된 ID로 바꿉니다. 에셋 명령에는 `--project`가 필수이고 `--ids`는 쉼표로 구분합니다.

```sh
taid --help
taid project create --parent "$PARENT" --name "$NAME"
taid project recent
taid project open "$PROJECT"
taid --project "$PROJECT" inspect
taid --project "$PROJECT" profile list
taid --project "$PROJECT" import "$INPUT" --profile generic
taid --project "$PROJECT" asset update "$ASSET_ID" --file "$PATCH_JSON"
taid --project "$PROJECT" profile put --file "$PROFILE_JSON"
taid --project "$PROJECT" notes --text "$NOTES"
taid --project "$PROJECT" cutout --ids "$ASSET_IDS"
taid --project "$PROJECT" palette inspect --asset "$ASSET_ID"
taid --project "$PROJECT" palette import --file "$PALETTE_FILE" --profile "$PROFILE_ID"
taid --project "$PROJECT" palette map --ids "$ASSET_IDS"
taid --project "$PROJECT" tile --ids "$ASSET_IDS" --width 32 --height 32 --scale 4 --mode pad --anchor bottom-center
taid --project "$PROJECT" process --ids "$ASSET_IDS" --snap --trim --scale 2
taid --project "$PROJECT" reset --ids "$ASSET_IDS"
taid settings get
taid settings set --model "$MODEL_ID" --effort medium
taid --project "$PROJECT" approve --ids "$ASSET_IDS" --design-reviewed --qc-reviewed
taid --project "$PROJECT" export --ids "$ASSET_IDS" --out "$DELIVERY_PARENT"
```

`process`는 요청한 플래그만 넣습니다. `--alpha`는 손실성 알파 이진화이고 preserve-alpha 이펙트 기준에서는 사용할 수 없습니다. `--palette`는 저장된 팔레트가 필요합니다. `--trim`은 전체 시퀀스를 선택해 상대 오프셋을 보존하는 union trim입니다. process 배율은 1–8, tile 배율은 1–64입니다. 타일 모드는 `pad|fit`, 정렬은 `center|bottom-center`입니다. settings 강도는 `none|minimal|low|medium|high|xhigh`이며 모델의 지원 여부도 확인하세요.

`profile put`은 완전한 camelCase 기준 JSON을, `asset update`는 필요한 필드만 담은 camelCase 패치를 읽습니다. 필드는 [워크벤치 스킬](skills/taid-workbench/SKILL.md)에 정리되어 있습니다. `palette import` CLI는 GUI 초안 읽기와 달리 해당 기준을 즉시 저장하므로 연결 에셋 승인에 영향을 줍니다.

성공은 stdout, 실패는 stderr에 `status`, `summary`, `data`, `artifacts`, `nextActions`를 가진 JSON 객체 하나를 출력합니다. 실패 종료 코드는 1이고 stdout은 비어 있습니다. 도움말·버전만 일반 텍스트입니다. `inspect`는 로컬 이미지 경로를 반환하며 이미지 바이트를 JSON에 넣지 않습니다. `project open`은 다음 GUI 실행의 프로젝트 선택을 바꾸고, 이미 열린 창은 다시 불러오기가 필요합니다.

Codex가 CLI로 작업할 때는 [taid-workbench 스킬](skills/taid-workbench/SKILL.md)을 사용합니다. 처리·내보내기 요청만으로 디자인 승인을 추론하지 마세요. 사용자가 대상 에셋의 실제 디자인/QC 검토와 승인을 명시적으로 요청한 경우에만 승인 명령을 실행합니다.

## 로컬 빌드와 설치

### macOS

Node/npm, Rust, Xcode Command Line Tools와 Tauri macOS 빌드 의존성이 필요합니다.

```sh
npm ci
npm run desktop:dev
# 배포용 앱과 CLI를 함께 빌드
npm run desktop:build -- --bundles app,dmg --no-sign --no-binary-patching -- --bins --locked
npm run install:mac
```

설치 스크립트는 빌드된 앱을 `~/Applications/Taid.app`으로 복사하고 `~/.local/bin/taid`, `~/.agents/skills/taid-workbench`를 연결합니다. PATH에 `~/.local/bin`을 추가하세요. 스킬과 서드파티 고지는 앱의 `Contents/Resources`에 포함되며 스킬 링크도 설치된 앱 내부를 가리킵니다. 실행 중인 기존 앱은 먼저 종료해야 하며, 관련 없는 앱이나 링크는 덮어쓰지 않습니다. 이전 앱은 캐시에 보존합니다. 위 명령은 Developer ID 서명·공증을 수행하지 않으며 기존 바이너리 서명을 깨는 패칭도 건너뜁니다. Gatekeeper나 조직 보안 설정을 우회하지 마세요.

### Windows x64

네이티브 Windows에서 Node/npm, Rust MSVC의 `x86_64-pc-windows-msvc` 타깃, Visual Studio C++ Build Tools와 Windows SDK가 필요합니다. 도구·라이선스는 사용자가 설치·승인해야 합니다. 링커를 찾지 못하면 Developer PowerShell을 사용하세요.

```powershell
npm ci
rustup target add x86_64-pc-windows-msvc
powershell -File scripts/build-windows.ps1
# NSIS 없이 portable만 만들 때
pwsh -File scripts/build-windows.ps1 -PortableOnly
```

NSIS는 `.local/windows-target/x86_64-pc-windows-msvc/release/bundle/nsis/`, portable GUI·CLI와 필요한 DLL, 고지·SHA-256 목록은 `.local/windows-target/portable/`에 생성됩니다. portable은 폴더 전체를 함께 옮기세요. GUI는 `taid-designer.exe`, CLI는 `taid.exe`입니다. portable GUI에는 WebView2 Runtime이 필요하며 사용자별 NSIS 설치기는 없을 때 다운로드 UI를 표시합니다. 현재 배포 구성은 **서명되지 않은** NSIS/portable입니다. SmartScreen 경고가 없다고 보장하지 않으며 출처·체크섬과 조직 정책을 확인하세요.

[Windows 워크플로](.github/workflows/windows.yml)는 네이티브 빌드·CLI/GUI/설치기 검사를 정의합니다. 워크플로 존재나 패키징 성공은 특정 실행의 통과 증거가 아닙니다. Windows 실행 결과는 해당 Actions 실행 로그와 아티팩트를 확인하세요.

## 검증 범위와 참고 자료

로컬 검증에는 직접 만든 합성 PNG만 사용했으며 회사 자료는 사용하지 않았습니다. 합성 이미지의 실제 ONNX 누끼에서 320px 캔버스, RGB 보존, 투명 배경과 전경 알파를 확인했고 CLI 타일 변형 생성도 확인했습니다. 이는 회사 에셋의 품질, 모든 배경의 분할 정확도, Windows 설치 성공을 보증하지 않습니다.

- [참고 영상: 픽셀아트 정리](https://youtube.com/shorts/OBuSY5YdM78): 불균일 격자, 과도한 색 수, 격자 정리 → 팔레트 → 수작업 잔점 정리 → 게임 내 크기 확인의 근거입니다. 영상의 9색 게임/14색 예시는 모든 프로젝트의 제한이 아닙니다.
- [U²-Net 원본](https://github.com/xuebinqin/U-2-Net), [논문](https://arxiv.org/abs/2005.09007), [U2Netp 모델 카드](https://huggingface.co/edgetools/u2netp).
- [고정 ONNX 다운로드](https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx), [가중치 Apache-2.0 라이선스](https://github.com/xuebinqin/U-2-Net/blob/master/LICENSE), [라이선스 구분 설명](https://github.com/danielgatis/rembg/issues/837#issuecomment-5172958667).
- [Codex CLI](https://github.com/openai/codex), [인증 문서](https://developers.openai.com/codex/auth/).
- [서드파티 고지](THIRD_PARTY_NOTICES.md), [누끼 배포 고지](src-tauri/src/cutout/NOTICE.txt).
