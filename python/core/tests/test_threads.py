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

# CPython's free-threaded builds, 3.14 and 3.15 at least, release a memoryview
# that threads slice at once, whose slices then raise ValueError, "operation
# forbidden on released memoryview object": python/cpython#127716
FREE_THREADED_MEMORYVIEWS = not sys._is_gil_enabled()
MEMORYVIEW_RACE = (
    "which CPython's free-threaded builds, 3.14 and 3.15 at least, release when "
    "threads slice it at once: https://github.com/python/cpython/issues/127716"
)


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
        FREE_THREADED_MEMORYVIEWS and tafla.IMPLEMENTATION == "python",
        "the pure-Python reader slices a string's bytes from the buffer's "
        f"memoryview, {MEMORYVIEW_RACE}",
    )
    def test_reads_the_same_strings_from_a_buffer_that_threads_share(self) -> None:
        buffer = shared_buffer()

        def read() -> None:
            for _ in range(2000):
                assert buffer.get_string(buffer.root_offset(1)) == "Europe/Stockholm"

        self.assertEqual(run_together(read), [])

    @unittest.skipIf(
        FREE_THREADED_MEMORYVIEWS,
        f"each reader slices an array from the buffer's memoryview, {MEMORYVIEW_RACE}",
    )
    def test_reads_the_same_arrays_from_a_buffer_that_threads_share(self) -> None:
        buffer = shared_buffer()

        def read() -> None:
            for _ in range(2000):
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

    @unittest.skipIf(
        tafla.IMPLEMENTATION == "python" and not sys._is_gil_enabled(),
        "array() reads a list item by item, which another thread changes "
        "between them on a free-threaded build",
    )
    def test_writes_a_list_as_it_is_at_once_while_another_thread_replaces_it(
        self,
    ) -> None:
        """A list's elements are written as they are at one moment, whatever
        another thread does to the list meanwhile: each state of this one
        holds a single value, and a length that the value gives"""
        values = [0.0] * 1000
        stop = threading.Event()

        def replace() -> None:
            count = 0
            while not stop.is_set():
                count += 1
                values[:] = [float(count)] * (1000 + count % 3)

        def write() -> None:
            for _ in range(200):
                writer = TableWriter(1)
                writer.set_float64_array(0, values)
                buffer = ByteBuffer(bytes(writer.bytes()))
                written = buffer.get_float64_array(buffer.root_offset(0))
                assert len(set(written)) == 1, "the elements of two states"
                assert len(written) == 1000 + int(written[0]) % 3, "another length"

        replacer = threading.Thread(target=replace)
        replacer.start()
        try:
            self.assertEqual(run_together(write), [])
        finally:
            stop.set()
            replacer.join()


if __name__ == "__main__":
    unittest.main()
