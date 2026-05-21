use std::collections::{HashSet, VecDeque};
use crate::models::{IdentityProfile, EntityNode, EntityType, SourceMetadata};

/// Статусы управления очередью
pub struct AnalysisEngine {
    /// Очередь задач на обработку (FIFO)
    task_queue: VecDeque<EntityNode>,
    /// Глобальный реестр уже проверенных значений для предотвращения зацикливания
    visited_pool: HashSet<String>,
    /// Итоговое аналитическое досье, которое мы наполняем в процессе
    pub final_profile: IdentityProfile,
}

impl AnalysisEngine {
    pub fn new(root_entity: EntityNode) -> Self {
        let mut task_queue = VecDeque::new();
        task_queue.push_back(root_entity.clone());

        let  final_profile = IdentityProfile {
            root_entity,
            associated_nodes: std::collections::HashMap::new(),
            active_links: Vec::new(),
            calculated_confidence: 0,
        };

        AnalysisEngine {
            task_queue,
            visited_pool: HashSet::new(),
            final_profile,
        }
    }

    /// Главный управляющий цикл автоматического пивотинга
    pub async fn resolve_cascade(&mut self) {
        while let Some(current_node) = self.task_queue.pop_front() {
            // Защита от бесконечного цикла: если сущность уже обрабатывалась, пропускаем
            if self.visited_pool.contains(&current_node.value) {
                continue;
            }

            self.visited_pool.insert(current_node.value.clone());
            println!("[Engine] Анализ узла: [{:?}] {}", current_node.entity_type, current_node.value);

            // ИМИТАЦИЯ ПОТОКА ДАННЫХ (в реальности здесь обращение к индексам ClickHouse или API)
            // Допустим, наш парсер извлек новые связи из сырого лога
            let mock_raw_line = "pavel_dev; +79991112233; pavel@mail.ru";
            let mock_source = SourceMetadata {
                source_id: "Internal_Secure_Index".to_string(),
                class: crate::models::SourceClass::PublicOSINT,
                import_timestamp: 1774123456,
                data_actual_year: 2026,
            };

            // Вызываем наш Шаг №2 (ETL Парсер)
            let (discovered_nodes, discovered_links) = crate::parser::parse_raw_line(mock_raw_line, &mock_source);

            // Интегрируем находки в профиль
            for node in discovered_nodes {
                if node.value != self.final_profile.root_entity.value {
                    // Если узел новый и обладает высоким приоритетом — планируем его рекурсивную проверку
                    if node.entity_type == EntityType::Phone || node.entity_type == EntityType::Email {
                        if !self.visited_pool.contains(&node.value) {
                            // Автоматическое расширение очереди поиска (Новый круг рекурсии)
                            self.task_queue.push_back(node.clone());
                        }
                    }
                    self.final_profile.associated_nodes.insert(node.value.clone(), node);
                }
            }

            self.final_profile.active_links.extend(discovered_links);

            // Вызываем наш Шаг №3 (Перерасчет скоринга на каждом круге)
            crate::scoring::evaluate_profile(&mut self.final_profile);

            println!("[Engine] Текущая достоверность графа: {}%", self.final_profile.calculated_confidence);
        }

        println!("[Engine] Каскадный анализ успешно завершен. Все связи распутаны.");
    }
}