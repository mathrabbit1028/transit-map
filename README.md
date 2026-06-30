# transit-map

대중교통 노선도 자동 제작 소프트웨어입니다.

1. site-checker는 node.js 앱으로, 지도에서 원하는 사이트를 쉽게 클릭하여 해당 위치의 위도와 경도를 얻을 수 있습니다. 이 정보는 노선도 제작에 사용됩니다.
2. route-svg는 rust 프로그램으로, site-checker에서 얻은 위치 데이터를 기반으로 SVG 형식의 노선도를 생성합니다. 이 노선도는 실제 위치를 기반으로 하며, 보기 좋은 노선도를 만들지는 않습니다.
3. route-optimizer는 rust 프로그램으로, site-checker에서 얻은 위치 데이터를 기반으로 최적화된 노선도를 생성합니다. 이 프로그램은 안정성을 위해 optimization step과 svg generation step을 분리하여 실행합니다.