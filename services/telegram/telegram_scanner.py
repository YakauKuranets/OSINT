import asyncio
import threading
from flask import Flask, request, jsonify
from telethon import TelegramClient
from telethon.tl.functions.users import GetFullUserRequest

app = Flask(__name__)

API_ID = 37617689
API_HASH = "903e68572eefcd827edceb5162d19605"

client = None
main_loop = None


async def init_client():
    global client
    client = TelegramClient("session", API_ID, API_HASH)
    await client.start()


async def search_public_chats(username: str):
    """Поиск username в публичных чатах и получение полного профиля."""
    results = []
    try:
        # 1. ЗАПРОС ПОЛНОГО ПРОФИЛЯ (Вытаскиваем телефон и Bio)
        try:
            entity = await client.get_entity(username)
            # Делаем расширенный запрос к серверам Telegram
            full_user = await client(GetFullUserRequest(entity))

            user_info = full_user.users[0]
            about = full_user.full_user.about or ""

            # Телетон часто отдает номер без '+'. Добавляем, чтобы Rust его распознал
            phone = getattr(user_info, "phone", "")
            if phone and not phone.startswith("+"):
                phone = "+" + phone

            results.append({
                "type": "user",
                "id": user_info.id,
                "username": getattr(user_info, "username", ""),
                "first_name": getattr(user_info, "first_name", ""),
                "last_name": getattr(user_info, "last_name", ""),
                "phone": phone,  # ТЕПЕРЬ НОМЕР ПЕРЕДАЕТСЯ РАСТУ
                "bio": about
            })
        except Exception as e:
            print(f"[!] Ошибка получения сущности: {e}")

        # 2. Поиск общих публичных групп/каналов
        dialogs = await client.get_dialogs(limit=50)
        groups_found = 0
        for dialog in dialogs:
            if dialog.is_group or dialog.is_channel:
                try:
                    participants = await client.get_participants(dialog, search=username, limit=1)
                    if participants:
                        groups_found += 1
                        results.append({
                            "type": "group_membership",
                            "chat_title": dialog.name,
                            "chat_id": dialog.id,
                            "participant_count": getattr(dialog, "participants_count", 0)
                        })
                        if groups_found >= 5:
                            break
                except:
                    continue

        # 3. Сбор последних сообщений
        for dialog in dialogs[:10]:
            try:
                messages = await client.get_messages(dialog, limit=50, search=username)
                for msg in messages:
                    results.append({
                        "type": "message",
                        "chat_title": dialog.name,
                        "sender_id": msg.sender_id,
                        "text": msg.text[:200] if msg.text else ""
                    })
            except:
                continue

    except Exception as e:
        results.append({"type": "error", "details": str(e)})

    return results


@app.route('/search', methods=['POST'])
def search():
    data = request.get_json(force=True, silent=True)
    if not data or 'username' not in data:
        return jsonify({"error": "Missing username"}), 400

    username = data['username'].strip()
    if not username:
        return jsonify({"error": "Empty username"}), 400

    # Потокобезопасная передача задачи в цикл Telethon
    try:
        future = asyncio.run_coroutine_threadsafe(search_public_chats(username), main_loop)
        results = future.result(timeout=30)
    except Exception as e:
        results = [{"error": str(e)}]

    return jsonify({"results": results})


def run_flask():
    # Запускаем Flask в отдельном потоке (отключаем reloader, чтобы не ломал asyncio)
    app.run(host='0.0.0.0', port=5002, debug=False, use_reloader=False)


if __name__ == '__main__':
    # 1. Создаем главный цикл для Telethon
    main_loop = asyncio.new_event_loop()
    asyncio.set_event_loop(main_loop)
    main_loop.run_until_complete(init_client())

    # 2. Запускаем Flask в фоновом потоке
    flask_thread = threading.Thread(target=run_flask)
    flask_thread.daemon = True
    flask_thread.start()

    print("[*] Telegram OSINT Microservice running on port 5002")

    # 3. Держим главный цикл активным для обработки сокетов Telegram
    main_loop.run_forever()