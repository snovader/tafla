"""Use from several threads at once, which free-threaded Python runs in
parallel: readers share a buffer, and a writer, which one thread at a time
may use, rejects another's call rather than racing it"""

import sys
import threading
import unittest
from array import array
from collections.abc import Callable

import tafla
from tafla import ByteBuffer, TableBuilder, TableLayout, TableWriter

THREADS = 8


def run_together(work: Callable[[], object]) -> list[BaseException]:
    """Runs ``work`` in each of several threads, started together, and
    returns what they raised"""
    barrier = threading.Barrier(THREADS)
    raised: list[BaseException] = []

    def run() -> None:
        barrier.wait()
        try:
            work()
        except BaseException as error:  # noqa: BLE001
            raised.append(error)

    threads = [threading.Thread(target=run) for _ in range(THREADS)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return raised


def shared_buffer() -> ByteBuffer:
    layout = TableLayout(sized=True, versioned=True)
    builder = TableBuilder(3, layout, 1)
    builder.set_float64(59.3293)
    builder.set_string("Europe/Stockholm")
    builder.set_float32_array(array("f", range(1000)))
    return ByteBuffer(builder.bytes(), layout)


class TestThreads(unittest.TestCase):
    def test_reads_the_same_values_from_a_buffer_that_threads_share(self) -> None:
        buffer = shared_buffer()

        def read() -> None:
            for _ in range(2000):
                assert buffer.get_float64(buffer.root_offset(0)) == 59.3293

        self.assertEqual(run_together(read), [])

    @unittest.skipIf(
        sys.version_info[:2] == (3, 14) and not sys._is_gil_enabled(),
        "CPython 3.14's free-threaded build releases a memoryview that threads "
        "slice at once, as the pure-Python reader slices strings and both "
        "slice arrays",
    )
    def test_reads_the_same_strings_and_arrays_from_a_buffer_that_threads_share(
        self,
    ) -> None:
        buffer = shared_buffer()

        def read() -> None:
            for _ in range(2000):
                assert buffer.get_string(buffer.root_offset(1)) == "Europe/Stockholm"
                assert buffer.get_float32_array(buffer.root_offset(2))[999] == 999

        self.assertEqual(run_together(read), [])

    @unittest.skipIf(
        tafla.IMPLEMENTATION == "python",
        "a pure-Python writer used by threads at once writes undefined bytes",
    )
    def test_rejects_a_call_to_a_writer_that_another_thread_is_using(self) -> None:
        """Each call borrows the writer, and another thread's call meanwhile
        raises RuntimeError; with the GIL, calls don't overlap"""
        writer = TableWriter(1)
        values = array("d", range(10_000))

        def write() -> None:
            for _ in range(50):
                try:
                    writer.start_table(1)
                    writer.set_float64_array(0, values)
                except RuntimeError:
                    pass

        self.assertEqual(run_together(write), [])
        self.assertGreater(len(writer.bytes()), 0)


if __name__ == "__main__":
    unittest.main()
