import subprocess
from flask import Flask, request, jsonify

app = Flask(__name__)


@app.route('/check_email', methods=['POST'])
def check_email():
    data = request.get_json(silent=True)
    if not data or 'email' not in data:
        return jsonify({"error": "Missing email"}), 400

    email = data['email'].strip()
    print(f"[*] Holehe начал проверку: {email} (это займет около 15 секунд...)")

    try:
        cmd = ["holehe", email, "--only-used", "--no-color"]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)

        found_sites = []
        for line in result.stdout.split('\n'):
            # Жесткий фильтр: берем только строки с [+] и исключаем легенду Holehe
            if "[+]" in line and "email used" not in line.lower() and "rate limit" not in line.lower():
                site_name = line.split("[+]")[1].strip()
                # Убираем ссылки и мусор, оставляем только имя (например, из http://.../spotify.com берем spotify)
                if "http" in site_name:
                    site_name = site_name.split(".")[-2]
                if site_name:
                    found_sites.append(site_name)

        print(f"[+] Найдено чистых регистраций: {len(found_sites)}")
        return jsonify({"registered": list(set(found_sites))})  # set() убирает дубликаты

    except Exception as e:
        print(f"[!] Ошибка сканирования: {e}")
        return jsonify({"error": str(e)}), 500


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5003, debug=False)