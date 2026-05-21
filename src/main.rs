mod models;
mod parser;
mod scoring;
mod engine; // Подключаем наш новый файл диспетчера

use models::{EntityNode, EntityType};

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("     📊 X-GEN OSINT ENGINE CORE v1.0 [RUNNING]    ");
    println!("==================================================\n");

    // 1. Задаем стартовую точку поиска (целевой никнейм БЕЗ пробелов)
    let start_target = EntityNode {
        value: "egor_egomostiev".to_string(), // Используй реальный никнейм для тестов
        entity_type: EntityType::Nickname,
        first_seen: 1774123456,
    };

    // 2. Инициализируем наш автомат состояний (Диспетчер)
    let mut osint_machine = engine::AnalysisEngine::new(start_target);

    // 3. Запускаем каскадный рекурсивный поиск
    osint_machine.resolve_cascade().await;

    // 4. На выходе получаем красивый готовый результат с оценкой
    println!("\n[+] Итоговый профиль собран со степенью доверия: {}%",
        osint_machine.final_profile.calculated_confidence
    );
}