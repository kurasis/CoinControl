// A dev-server-only component fixture. It is never imported by the production entry point.
import { createRoot } from "react-dom/client";
import "../../src/styles/tokens.css";
import "../../src/styles/app.css";
import { WindowedTableBody } from "../../src/components/WindowedTableBody";

const rows = Array.from({ length: 10000 }, (_, id) => ({ id }));
createRoot(document.getElementById("root")!).render(
  <div className="table-scroll" aria-label="Long table">
    <table className="table">
      <thead>
        <tr>
          <th>Row</th>
          <th>Variable height content</th>
        </tr>
      </thead>
      <WindowedTableBody rows={rows} columns={2} rowKey={(row) => row.id}>
        {(row) => (
          <>
            <td>
              <button className="btn">Row {row.id}</button>
            </td>
            <td className="wrap">
              {row.id % 5 === 0
                ? "A wrapped description with multiple lines. ".repeat(8)
                : "Short description"}
            </td>
          </>
        )}
      </WindowedTableBody>
    </table>
  </div>,
);
