# route-optimizer

`route-optimizer`는 실제 정류장 위치와 `data/route/*.json`의 노선 정의를 읽어 octilinear 노선도를 만든다. 최적화 단계와 SVG 생성 단계는 분리되어 있고, 중간 결과는 JSON으로 저장된다.

## 구조

- `src/main.rs`: CLI, 파일 입출력, `all | optimize | svg` 단계 실행
- `src/optimizer.rs`: 좌표 투영, 중간 JSON 스키마, cost function, backprop/Adam, SVG 렌더링

Rust 소스는 위 두 파일만 사용한다.

## 실행

```bash
cd route-optimizer
cargo run -- \
  --route ../data/route/{number}.json \
  --position ../data/position \
  --intermediate ../optimized-map.json \
  --output ../optimized-map.svg
```

단계를 나눠 실행할 수도 있다.

```bash
cargo run -- --step optimize \
  --route ../data/route/{number}.json \
  --position ../data/position \
  --intermediate ../optimized-map.json

cargo run -- --step svg \
  --intermediate ../optimized-map.json \
  --output ../optimized-map.svg
```

## 중간 저장 포맷

중간 JSON은 `transit-map.route-optimizer.v1` 스키마 문자열을 갖는다.

- `canvas`, `bounds`: 실제 위경도를 SVG 좌표계로 투영하기 위한 정보
- `routes[].stops[].point`: 노선별 정류장 방문 좌표
- `stops[].projected`: 실제 위치 기반 초기 투영 좌표
- `stops[].optimized`: 같은 정류장에 속한 노선별 노드들의 중심 좌표
- `stops[].visits[]`: 같은 정류장에 대응되는 노선별 개별 노드 좌표
- `optimization.initial_cost`, `optimization.final_cost`: 최적화 전후 cost breakdown
- `optimization.weights`, `optimization.parameters`: cost function 조정값

## 최적화 원칙

좌표는 실제 위치 projection에서 시작한다. 이후 reverse-mode autodiff tape로 cost gradient를 계산하고 Adam으로 좌표를 업데이트한다. 별도의 projected optimization이나 하드 스냅 단계는 두지 않았다. 레이아웃 규칙은 가능한 한 `impl Problem` 안의 cost term으로 유도한다.

동일 입력에 대해서는 항상 같은 결과가 나오도록 정렬 기준과 초기 offset은 결정론적으로 고정되어 있다. 난수는 사용하지 않는다.

SVG 렌더링 단계에서는 최종 좌표 사이를 8방향 octilinear connector로 그린다. 두 노드가 수평, 수직, 45도 대각선으로 바로 이어질 수 있으면 0-bend로 연결하고, 아니면 가능한 1-bend 후보 중 가장 짧은 경로를 선택한다.

## Cost Function

`src/optimizer.rs`의 `impl Problem` 안에 cost term이 함수별로 분리되어 있다.

- `anchor_cost`: 실제 위치 기반 좌표와의 거리
- `octilinear_cost`: 각 segment가 수평, 수직, 45도 대각선에 가까운 정도
- `direction_preference_cost`: 불필요한 방향 전환과 되돌아가는 흐름 억제
- `bend_cost`: 굴곡 자체와 너무 짧은 bend leg 억제
- `bend_angle_preference_cost`: 내부 꺾임각이 45도보다 90도, 90도보다 135도에 가깝도록 선호
- `overlap_cost`: 관계없는 segment끼리 같은 픽셀을 차지하지 않도록 하는 항
- `self_crossing_cost`: 동일 노선 내부 자가 교차 억제
- `shared_corridor_bundle_cost`: 같은 정류장쌍을 잇는 여러 노선을 병렬 lane으로 정렬
- `shared_stop_compactness_cost`: 같은 정류장의 노선별 노드들을 가까이 유지
- `shared_stop_lane_gap_cost`: 환승역의 겹친 원들이 일정 간격으로 일부 겹치도록 유도
- `transfer_alignment_cost`: 환승역 노드들을 가로, 세로, 양의 대각선, 음의 대각선 중 한 축으로 강하게 정렬
- `shared_segment_order_cost`: 공용 구간 안에서 노선의 상대적 순서 유지
- `stop_spacing_cost`: 서로 다른 정류장이 붕괴하지 않도록 분리
- `station_line_clearance_cost`: 관계없는 노선 선이 정류장 원을 관통하지 않도록 분리
- `label_spacing_cost`: 라벨끼리, 라벨과 정류장 원 사이의 충돌 억제
- `segment_length_cost`: 너무 짧은 segment 방지
- `bounds_cost`: 캔버스 바깥 이탈 방지

가중치는 `CostWeights`, 거리 기준값은 `CostParameters`의 `Default`에서 조정한다. 실제 위치에서 더 벗어나도 괜찮으면 `anchor`를 낮춘다. 서로 다른 노선의 교차는 전용 cost로 0을 강제하지 않고, 관계없는 선분끼리 너무 붙는 상황만 `overlap_cost`로 부드럽게 억제한다. 동일 노선 자가교차를 더 강하게 막고 싶으면 `self_crossing`과 `self_crossing_clearance`를 올린다. 환승역 원의 겹침 정도는 `shared_lane_gap`으로 조정한다.

환승역 정렬은 projection이 아니라 exact-penalty에 가까운 loss로 처리한다. 현재 좌표에서 가장 잘 맞는 4개 축 중 하나를 고르고, 각 노드가 그 축 위의 등간격 target에서 벗어난 거리 `d`에 대해 `d^2 + transfer_alignment_hardness * d^4 / shared_lane_gap^2`를 부과한다. `transfer_alignment`는 이 항 전체의 가중치이고, `transfer_alignment_hardness`는 많이 벗어난 점을 더 가파르게 끌어오는 정도다.

환승역 내부의 원 순서는 연속 좌표가 아니라 permutation이라서 gradient만으로 바꾸지 않는다. 대신 각 환승역에서 가능한 순서를 결정론적으로 탐색하고, 그 순서로 원을 놓았을 때 주변 노선 팔들이 덜 교차하고 덜 가까워지는 local score를 최소화한다. `transfer_order_search_limit` 이하의 환승역은 전체 permutation을 보고, 더 큰 환승역은 인접 swap hill-climb를 사용한다. 최적화 중에도 `transfer_order_update_interval`마다 현재 좌표 기준으로 순서를 다시 고른다.

cost term 자체를 바꾸고 싶으면 해당 함수를 수정하거나 새 함수를 추가한 뒤 `cost_terms()`, `weighted_total()`, `breakdown_from_vars()`에 연결하면 된다.

## 환승역과 공용 구간

환승역은 정류장을 하나로 합치지 않는다. 노선별 정류장 노드는 모두 독립 좌표로 유지하고, 같은 정류장에 속한 노드들이 서로 가까이 일부 겹치도록 cost를 준다. 정렬 축은 현재 좌표에서 가장 cost가 낮은 가로, 세로, 양의 대각선, 음의 대각선을 선택한다.

공용 구간도 노선을 합치지 않는다. 예를 들어 `A-B-C`를 세 노선이 공유하면 A, B, C 각각에서 세 개의 원이 유지된다. 같은 정류장쌍을 잇는 segment들은 `shared_corridor_bundle_cost`와 `shared_segment_order_cost`로 병렬 lane을 유지한다.

## SVG 후처리

SVG 단계는 새 최적화를 하지 않는다. 중간 JSON의 좌표를 읽고, 각 노선의 연속 정류장쌍을 최소 bend octilinear path로 그린다. 범례는 노선 수에 맞춰 여러 줄로 늘어나며 모든 노선을 표시한다. 정류장명 라벨은 선과 원을 모두 그린 뒤 마지막에 배치하고, 후보 위치 중 노선 선분, 정류장 원, 이미 놓인 라벨과 덜 겹치는 위치를 선택한다.
