# route-svg

`data/route/*.json`(노선 정의) + `data/position/*.json`(정류장 좌표) 를 읽어서, **부드러운(rounded) SVG 노선도**를 생성하는 Rust CLI입니다.

## 입력 형식

### 노선 파일 예시 (`data/route/5513_1.json`)

```json
[
  {
    "id": "5513-1",
    "name": "5513(종점행)",
    "route": ["31001", "01009"],
    "style": { "color": "#039696", "type": "-" }
  }
]
```

- `route`: 정류장/지점 id 배열
- `style.color`: 선 색
- `style.type`: `"-"` 또는 `"--"`(점선)

### 위치 파일 예시 (`data/position/01.json`)

```json
[{ "id": "01009", "name": "정문", "lat": 37.46, "lng": 126.94 }]
```

## 실행

```bash
cd route-svg
cargo run --release -- --routes ../data/route --positions ../data/position --out ../out.svg
```

옵션:

- `--size 1200` : 캔버스 크기(px)
- `--padding 60` : 여백(px)
- `--stroke-width 10` : 노선 두께
- `--smooth 0.22` : 코너 라운딩 강도(0..1)
- `--label-ids` : 정류장 id 라벨 표시

## 출력

기본 출력은 `../out.svg` 입니다.

브라우저에서 열거나, 벡터 편집기(Illustrator/Inkscape)로 확인할 수 있어요.

## 참고

이건 ‘지도 기반 실제형’이 아니라, lat/lng를 간단 투영(equirectangular)해서 **전체를 캔버스에 맞게 스케일**하는 방식이라,
도시/캠퍼스 범위에서는 충분히 보기 좋은 결과를 냅니다.
