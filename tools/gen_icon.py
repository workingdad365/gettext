"""GetText 아이콘 생성 스크립트 (assets/gettext.ico, assets/gettext.png)"""
from pathlib import Path

from PIL import Image, ImageDraw

S = 1024
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets"


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def render() -> Image.Image:
    # 대각선 그라데이션 배경
    top, bottom = (16, 185, 129), (13, 110, 160)
    grad = Image.new("RGBA", (S, S))
    px = grad.load()
    for y in range(S):
        for x in range(S):
            px[x, y] = lerp(top, bottom, (x + y) / (2 * S)) + (255,)

    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), radius=210, fill=255)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)

    # 문서 카드
    d.rounded_rectangle((200, 190, 824, 834), radius=70, fill=(255, 255, 255, 70))
    d.rounded_rectangle((180, 170, 804, 814), radius=70, fill=(255, 255, 255, 255))

    # 텍스트 줄과 선택 영역
    lines = [(250, 270, 730), (250, 390, 690), (250, 510, 734), (250, 630, 560)]
    d.rounded_rectangle((232, 372, 708, 448), radius=18, fill=(253, 224, 71, 255))
    for x0, y, x1 in lines:
        d.rounded_rectangle((x0, y, x1, y + 40), radius=20, fill=(30, 64, 90, 255))

    # 마우스 커서
    cursor = [(600, 520), (600, 900), (690, 815), (750, 950), (815, 920), (755, 790), (870, 790)]
    shadow = [(x + 16, y + 16) for x, y in cursor]
    d.polygon(shadow, fill=(10, 40, 60, 120))
    d.polygon(cursor, fill=(20, 20, 30, 255))
    inner = [(625, 580), (625, 840), (695, 775), (760, 905), (790, 892), (725, 765), (815, 765)]
    d.polygon(inner, fill=(255, 255, 255, 255))
    return img


def main():
    OUT.mkdir(exist_ok=True)
    big = render()
    big.resize((256, 256), Image.LANCZOS).save(OUT / "gettext.png")
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    big.resize((256, 256), Image.LANCZOS).save(OUT / "gettext.ico", sizes=[(s, s) for s in sizes])
    print("generated", OUT)


if __name__ == "__main__":
    main()
