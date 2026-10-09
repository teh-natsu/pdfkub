<p align="center">
  <img src="assets/app-icon/pdfkub.svg" alt="ไอคอน PdfKub: หน้ากระดาษกับกล่องคำพูดสีส้มเขียนว่า ครับ" width="128">
</p>

<h1 align="center">PdfKub</h1>

<p align="center">
  <b>โปรแกรมจัดการ PDF แบบโอเพนซอร์ส เขียนด้วย Rust ทั้งหมด</b><br>
  อ่าน ค้นหา จัดหน้า รวม แยก ใส่คอมเมนต์ กรอกฟอร์ม ลงลายเซ็น และป้องกันไฟล์ PDF<br>
  macOS · Windows · Linux · FreeBSD · เว็บ
</p>

<p align="center">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-5a4ff0">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-2d2391">
  <img alt="No account, no telemetry" src="https://img.shields.io/badge/no%20account-no%20telemetry-ff7417">
</p>

> [!NOTE]
> PdfKub พัฒนาต่อจาก [PdfCraft](https://github.com/storytold/pdfcraft) ของทีม ArtCraft
> ภายใต้สัญญาอนุญาต MIT OR Apache-2.0 แต่ไม่ได้จัดทำ สนับสนุน หรือรับรองโดยทีม ArtCraft

---

## จุดเด่น

- **อ่านได้สวย:** รองรับภาษาไทยและอักษรอื่น ๆ ทั่วโลก ตัวอักษรแนวตั้งภาษาญี่ปุ่น อีโมจิสี และการไล่สี
- **บันทึกปลอดภัย:** บันทึกแบบต่อท้ายไฟล์ (incremental) โดยไม่แตะข้อมูลเดิม เขียนแบบ atomic และย้อนกลับ (undo) ได้หลายขั้น
- **เป็นของคุณ:** ไม่ต้องสมัครบัญชี ไม่ส่งข้อมูลการใช้งาน ทำงานแบบออฟไลน์ได้ทั้งหมด
- **ทนไฟล์เสีย:** แต่ละหน้าแสดงผลแยกกัน และซ่อมไฟล์ที่เสียหายได้

## ทำอะไรได้บ้าง

| งาน | รายละเอียด |
|---|---|
| อ่านและค้นหา | ค้นหาขณะพิมพ์ เลือกและคัดลอกข้อความตามลำดับการอ่าน บุ๊กมาร์ก ภาพย่อ และป้ายหน้า (i, ii, 1, 2…) |
| จัดหน้า | หมุน ลบ แทรกหน้าว่าง แทรกหน้าจากไฟล์อื่น และย้ายลำดับหน้า |
| รวมและแยก | รวมหลายไฟล์ ดึงบางหน้าออก หรือแยกทุก *n* หน้า โดยลิงก์ ฟอร์ม เลเยอร์ และไฟล์แนบยังอยู่ครบ |
| ความปลอดภัย | เปิดไฟล์ที่เข้ารหัสได้ทุกแบบตั้งแต่ RC4 40 บิต ถึง AES-256 และทำ redaction |
| คอมเมนต์และฟอร์ม | ไฮไลต์ โน้ต รูปทรง การตอบกลับ กรอกและสร้างฟอร์ม (JavaScript ทำงานใน sandbox) |
| ลายเซ็น | ลงลายเซ็นดิจิทัล PAdES B-B และตรวจสอบลายเซ็น |

### ใช้จาก command line

```sh
pdfkub-cli combine report.pdf appendix.pdf --out combined.pdf
pdfkub-cli extract report.pdf --pages 1,3,5 --out highlights.pdf
pdfkub-cli split   report.pdf --every 10 --out-dir parts/
```

### ให้ AI agent ควบคุม

ทุกฟีเจอร์ของ engine เรียกใช้ได้โดยไม่ต้องเปิดหน้าจอ ผ่านตารางเครื่องมือชุดเดียวกัน:

- **`pdfkub-cli run`**: เรียกครั้งเดียว หรือรันสคริปต์ JSON

  ```sh
  pdfkub-cli tools                                  # รายการเครื่องมือทั้งหมดพร้อม JSON Schema
  pdfkub-cli run text_find doc=1 query=invoice
  pdfkub-cli run --script steps.json --root ./work  # จำกัดไฟล์ที่อ่าน/เขียนไว้ในโฟลเดอร์เดียว
  ```

- **MCP server** (ต้องเปิดเอง): ทำงานผ่าน stdin/stdout เท่านั้น ไม่เปิดพอร์ตเครือข่าย

  ```json
  { "mcpServers": { "pdfkub": { "command": "pdfkub-cli", "args": ["mcp", "--root", "/path/to/your/pdfs"] } } }
  ```

- **Rust API** (`pdfcraft_automation::Automation::call`) สำหรับฝังในโปรแกรมอื่น

การแก้ไขจะอยู่ในหน่วยความจำและย้อนกลับได้จนกว่าจะเรียก `doc_save`

## โครงสร้างโค้ด

เป็น Cargo workspace ที่แบ่ง crate ตามหน้าที่ โดยแกนหลักไม่ขึ้นกับ UI
(ชื่อ crate ภายในยังเป็น `pdfcraft-*` เหมือนต้นฉบับ เพื่อให้ดึงอัปเดตจาก PdfCraft ได้ง่าย):

| Crate | หน้าที่ |
|---|---|
| `pdfcraft-filters` | ตัวกรอง stream ของ PDF ทั้งหมด (Flate, LZW, ASCII85, RunLength, predictor) |
| `pdfcraft-crypt` | ระบบความปลอดภัยมาตรฐาน: RC4, AES-128/256, revision 2–6, สิทธิ์การใช้งาน |
| `pdfcraft-cos` | ชั้น object ของ PDF: parse แบบยืดหยุ่น ซ่อมไฟล์ แก้แบบ copy-on-write และเขียนไฟล์ |
| `pdfcraft-organize` | จัดหน้า รวม ดึง แยก บุ๊กมาร์ก ป้ายหน้า ข้อมูลเอกสาร |
| `pdfcraft-annot` | คอมเมนต์และ appearance stream |
| `pdfcraft-forms` | ฟอร์มแบบโต้ตอบ |
| `pdfcraft-render` | แสดงผล ตรวจสอบ และดึงข้อความตามลำดับการอ่าน |
| `pdfcraft-engine` | ส่วนกลางที่ทุก frontend ใช้: session, การแก้ไข, undo, การบันทึก |
| `pdfcraft-automation` | เครื่องมือแบบ headless, `pdfkub-cli run` และ MCP server |
| `pdfcraft-ui-egui` | หน้าจอ desktop และเว็บ |
| `apps/pdfkub`, `apps/pdfkub-cli`, `apps/pdfkub-web` | ตัวโปรแกรม, CLI และเวอร์ชันเว็บ |

## เริ่มต้นใช้งาน

ต้องมี Rust 1.90 ขึ้นไป (บน Windows ต้องมี Visual Studio Build Tools ที่มี C++ ด้วย)

```sh
git clone https://github.com/teh-natsu/pdfkub
cd pdfkub
cargo run --release -p pdfkub -- some.pdf      # เปิดโปรแกรม
cargo test                                      # รันเทสต์
cargo xtask ci                                  # ตรวจทุกอย่างแบบเดียวกับ CI
```

ภาษาของหน้าจอเลือกได้ที่ **Menu → Edit → Preferences…** (ดู [docs/localization.md](docs/localization.md))
บันทึกการพัฒนา, log และตัวแปรสภาพแวดล้อมอยู่ใน [docs/development.md](docs/development.md)

## สถานะ

ยังเป็นรุ่นทดลอง (early beta) ใช้ดู จัดหน้า รวม/แยก คอมเมนต์ และกรอกฟอร์มได้ดีแล้ว
ส่วนที่ยังอ่อนคือการแก้ข้อความเดิมในไฟล์ OCR ภาษาที่ไม่ใช่ละติน (ยังไม่รองรับภาษาไทย)
การนำเข้า/ส่งออกไฟล์ Office และ timestamp ของลายเซ็น รายละเอียดอยู่ใน [ROADMAP.md](ROADMAP.md)

## สัญญาอนุญาตและเครดิต

PdfKub ใช้สัญญาอนุญาตคู่ [MIT](LICENSE-MIT) หรือ [Apache-2.0](LICENSE-APACHE) เลือกได้ตามต้องการ
Copyright (c) 2026 Nattpol Chaisri and the PdfKub contributors

พัฒนาต่อจาก [PdfCraft](https://github.com/storytold/pdfcraft),
Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors ข้อความที่ต้องแสดงอยู่ใน [NOTICE](NOTICE)

ฟอนต์ ไอคอน และ asset อื่น ๆ ใช้สัญญาอนุญาตแบบเปิดของแต่ละชิ้น รายการพร้อมผู้สร้างและแหล่งที่มาอยู่ใน
[ATTRIBUTION.md](ATTRIBUTION.md) ไอคอนของโปรแกรมอธิบายไว้ใน [assets/app-icon/README.md](assets/app-icon/README.md)

<sub>Adobe and Acrobat are trademarks or registered trademarks of Adobe Inc. in the United States and/or other countries. PdfKub is an independent, open-source project and is not affiliated with, sponsored by or endorsed by Adobe Inc.</sub>
