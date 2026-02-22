# route-svg

`data/route/*.json`(노선 정의) + `data/position/*.json`(정류장 좌표) 를 읽어서, **실제위치 기반 SVG 노선도**를 생성하는 Rust CLI입니다.

## 입력 형식

### 노선 파일 예시 (`data/route/5513.json`)

```json
[
  {
    "id": "5513-1",
    "name": "5513(종점행)",
    "route": ["31001", "01009"],
    "style": { "color": "#039696", "bidirectional": false }
  }
]
```
  
- `route`: 정류장/지점 id 배열
- `style.color`: 선 색
- `style.bidirectional`: 양방향 여부

### 위치 파일 예시 (`data/position/01.json`)

```json
[{ "id": "01009", "name": "정문", "lat": 37.46, "lng": 126.94 }]
```

## 실행

```bash
cd route-svg
cargo run -- --route ../data/route/{number}.json --position ../data/position --output ../out.svg
```

## 출력

브라우저에서 열거나, 벡터 편집기(Illustrator/Inkscape)로 확인할 수 있어요.
