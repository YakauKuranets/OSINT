import asyncio
from flask import Flask, request, jsonify
from telethon import TelegramClient
from telethon.tl.functions.channels import GetParticipantsRequest
from telethon.tl.types import ChannelParticipantsSearch

app = Flask(__name__)

API_ID = 37617689
API_HASH = "903e68572eefcd827edceb5162d19605"

client = None

async def init_client():
    global client
    client = TelegramClient("session", API_ID, API_HASH)
    await client.start()

async def search_public_chats(username: str):
    """Поиск username в публичных чатах и каналах, включая общие группы и историю сообщений."""
    results = []
    try:
        # 1. Поиск самого аккаунта
        try:
            entity = await client.get_entity(username)
            if entity:
                results.append({
                    "type": "user",
                    "id": entity.id,
                    "username": getattr(entity, "username", ""),
                    "first_name": getattr(entity, "first_name", ""),
                    "last_name": getattr(entity, "last_name", ""),
                    "phone": getattr(entity, "phone", None)
                })
        except:
            pass

        # 2. Поиск общих публичных групп/каналов (где участвует username)
        dialogs = await client.get_dialogs(limit=50)  # сканируем больше диалогов
        groups_found = 0
        for dialog in dialogs:
            if dialog.is_group or dialog.is_channel:
                try:
                    # Ищем участника по username в этой группе
                    participants = await client.get_participants(dialog, search=username, limit=1)
                    if participants:
                        groups_found += 1
                        results.append({
                            "type": "group_membership",
                            "chat_title": dialog.name,
                            "chat_id": dialog.id,
                            "participant_count": getattr(dialog, "participants_count", 0)
                        })
                        if groups_found >= 5:  # ограничим количество, чтобы не спамить
                            break
                except:
                    continue

        # 3. Сбор последних сообщений из публичных чатов, где упоминается username
        for dialog in dialogs[:10]:  # первые 10 диалогов
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

    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    try:
        results = loop.run_until_complete(search_public_chats(username))
    except Exception as e:
        results = [{"error": str(e)}]
    finally:
        loop.close()

    return jsonify({"results": results})

if __name__ == '__main__':
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    loop.run_until_complete(init_client())
    app.run(host='0.0.0.0', port=5002, debug=False)